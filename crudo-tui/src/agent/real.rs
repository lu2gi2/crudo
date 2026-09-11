use super::events::AgentEvent;
use super::Agent;
use api::{
    max_tokens_for_model, resolve_model_alias, ContentBlockDelta, InputContentBlock, InputMessage,
    MessageRequest, OutputContentBlock, ProviderClient, StreamEvent, ToolChoice, ToolDefinition,
    ToolResultContentBlock,
};
use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ConfigLoader, ContentBlock, ConversationMessage,
    ConversationRuntime, MessageRole, PermissionMode, PermissionPolicy, PermissionPromptDecision,
    PermissionPrompter, PermissionRequest, RuntimeError, Session, ToolError, ToolExecutor,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tools::GlobalToolRegistry;

/// Resolves the model to use from environment variables, Crudo configuration, or default.
pub fn resolve_configured_model() -> String {
    // 1. Environment variables
    for var_name in ["CRUDO_MODEL", "ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_MODEL"] {
        if let Ok(val) = std::env::var(var_name) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    // 2. Crudo configuration loader from current directory
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(config) = ConfigLoader::default_for(&cwd).load() {
            if let Some(model) = config.model() {
                let trimmed = model.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }

    // 3. Fallback default model
    "claude-sonnet-4-6".to_string()
}

/// The list of tool names allowed under the "core" tool profile.
pub const CORE_PROFILE_TOOLS: &[&str] = &["read_file", "write_file", "edit_file", "bash"];

/// Filters model-visible tool definitions according to the `CRUDO_TOOL_PROFILE` environment variable.
///
/// When `CRUDO_TOOL_PROFILE` is set to `"core"` (case-insensitive, trimmed), only the four core tools
/// (`read_file`, `write_file`, `edit_file`, `bash`) are retained, preserving their original order.
/// If absent, empty, or set to any other value, all tool definitions are returned unmodified.
pub fn filter_tool_definitions(tools: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    filter_tool_definitions_with_lookup(tools, |k| std::env::var(k).ok())
}

/// Helper that accepts an environment lookup closure to enable deterministic unit testing without
/// modifying process-wide environment variables.
pub fn filter_tool_definitions_with_lookup<F>(
    tools: Vec<ToolDefinition>,
    mut env_lookup: F,
) -> Vec<ToolDefinition>
where
    F: FnMut(&str) -> Option<String>,
{
    match env_lookup("CRUDO_TOOL_PROFILE").as_deref().map(str::trim) {
        Some(profile) if profile.eq_ignore_ascii_case("core") => tools
            .into_iter()
            .filter(|tool| CORE_PROFILE_TOOLS.contains(&tool.name.as_str()))
            .collect(),
        _ => tools,
    }
}

/// Generates a concise summary from tool input parameters for the TUI activity panel.
fn summarize_tool_input(_tool_name: &str, input_json: &serde_json::Value) -> String {
    if let Some(path) = input_json.get("path").and_then(|v| v.as_str()) {
        return path.to_string();
    }
    if let Some(cmd) = input_json.get("command").and_then(|v| v.as_str()) {
        return cmd.to_string();
    }
    if let Some(pat) = input_json.get("pattern").and_then(|v| v.as_str()) {
        return pat.to_string();
    }
    if let Some(query) = input_json.get("query").and_then(|v| v.as_str()) {
        return query.to_string();
    }
    if let Some(url) = input_json.get("url").and_then(|v| v.as_str()) {
        return url.to_string();
    }
    if let Some(skill) = input_json.get("skill").and_then(|v| v.as_str()) {
        return skill.to_string();
    }
    if let Some(obj) = input_json.as_object() {
        if let Some((_, val)) = obj.iter().next() {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Converts runtime conversation messages to API input messages for model requests.
fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    let mut input_messages: Vec<InputMessage> = Vec::new();

    for message in messages {
        let role = match message.role {
            MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
            MessageRole::Assistant => "assistant",
        };
        let content = message
            .blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                ContentBlock::Thinking {
                    thinking,
                    signature,
                } => InputContentBlock::Thinking {
                    thinking: thinking.clone(),
                    signature: signature.clone(),
                },
                ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: serde_json::from_str(input)
                        .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                },
                ContentBlock::ToolResult {
                    tool_use_id,
                    output,
                    is_error,
                    ..
                } => InputContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text {
                        text: output.clone(),
                    }],
                    is_error: *is_error,
                },
            })
            .filter(|block| !matches!(block, InputContentBlock::Text { text } if text.is_empty()))
            .collect::<Vec<_>>();

        if content.is_empty() {
            continue;
        }

        // If consecutive messages have the same role (e.g. system/user/tool mapping or consecutive prompts),
        // merge their content blocks so providers requiring strictly alternating turns (like Anthropic)
        // receive a valid message sequence.
        if let Some(last) = input_messages.last_mut() {
            if last.role == role {
                last.content.extend(content);
                continue;
            }
        }

        input_messages.push(InputMessage {
            role: role.to_string(),
            content,
        });
    }

    input_messages
}

/// Helper to handle output blocks when starting or streaming blocks.
async fn push_output_block(
    block: OutputContentBlock,
    block_index: u32,
    events: &mut Vec<AssistantEvent>,
    pending_tools: &mut BTreeMap<u32, (String, String, String)>,
    pending_thinking: &mut BTreeMap<u32, (String, Option<String>)>,
    streaming_tool_input: bool,
    tx: &mpsc::Sender<(usize, AgentEvent)>,
    run_id: usize,
    cancel_rx: &watch::Receiver<bool>,
) {
    if *cancel_rx.borrow() {
        return;
    }
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() && !*cancel_rx.borrow() {
                let _ = tx.send((run_id, AgentEvent::TextChunk(text.clone()))).await;
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            pending_tools.insert(block_index, (id, name, initial_input));
        }
        OutputContentBlock::Thinking {
            thinking,
            signature,
        } => {
            if !*cancel_rx.borrow() {
                let _ = tx.send((run_id, AgentEvent::Thinking)).await;
            }
            if streaming_tool_input {
                pending_thinking.insert(block_index, (thinking, signature));
            } else {
                events.push(AssistantEvent::Thinking {
                    thinking,
                    signature,
                });
            }
        }
        OutputContentBlock::RedactedThinking { .. } => {}
    }
}

/// Streams assistant events from ProviderClient and feeds TextChunk events to the TUI.
async fn stream_with_provider(
    client: &ProviderClient,
    message_request: &MessageRequest,
    tx: &mpsc::Sender<(usize, AgentEvent)>,
    run_id: usize,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<Vec<AssistantEvent>, api::ApiError> {
    if *cancel_rx.borrow() {
        return Err(api::ApiError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Streaming cancelled",
        )));
    }

    let stream_fut = client.stream_message(message_request);
    let mut stream = tokio::select! {
        res = stream_fut => res?,
        _ = cancel_rx.changed() => {
            return Err(api::ApiError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Streaming cancelled",
            )));
        }
    };

    let mut events = Vec::new();
    let mut pending_tools: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
    let mut pending_thinking: BTreeMap<u32, (String, Option<String>)> = BTreeMap::new();
    let mut saw_stop = false;

    while !*cancel_rx.borrow() {
        let event_opt = tokio::select! {
            res = stream.next_event() => res?,
            _ = cancel_rx.changed() => {
                return Err(api::ApiError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Streaming cancelled",
                )));
            }
        };

        if *cancel_rx.borrow() {
            return Err(api::ApiError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Streaming cancelled",
            )));
        }

        let Some(event) = event_opt else {
            break;
        };

        match event {
            StreamEvent::MessageStart(start) => {
                for block in start.message.content {
                    if *cancel_rx.borrow() {
                        return Err(api::ApiError::Io(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "Streaming cancelled",
                        )));
                    }
                    push_output_block(
                        block,
                        0,
                        &mut events,
                        &mut pending_tools,
                        &mut pending_thinking,
                        true,
                        tx,
                        run_id,
                        &cancel_rx,
                    )
                    .await;
                }
            }
            StreamEvent::ContentBlockStart(start) => {
                if *cancel_rx.borrow() {
                    return Err(api::ApiError::Io(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Streaming cancelled",
                    )));
                }
                push_output_block(
                    start.content_block,
                    start.index,
                    &mut events,
                    &mut pending_tools,
                    &mut pending_thinking,
                    true,
                    tx,
                    run_id,
                    &cancel_rx,
                )
                .await;
            }
            StreamEvent::ContentBlockDelta(delta) => match delta.delta {
                ContentBlockDelta::TextDelta { text } => {
                    if !text.is_empty() && !*cancel_rx.borrow() {
                        let _ = tx.send((run_id, AgentEvent::TextChunk(text.clone()))).await;
                        events.push(AssistantEvent::TextDelta(text));
                    }
                }
                ContentBlockDelta::InputJsonDelta { partial_json } => {
                    if let Some((_, _, input)) = pending_tools.get_mut(&delta.index) {
                        input.push_str(&partial_json);
                    }
                }
                ContentBlockDelta::ThinkingDelta { thinking } => {
                    if !*cancel_rx.borrow() {
                        let _ = tx.send((run_id, AgentEvent::Thinking)).await;
                    }
                    if let Some((pending, _)) = pending_thinking.get_mut(&delta.index) {
                        pending.push_str(&thinking);
                    }
                }
                ContentBlockDelta::SignatureDelta { signature } => {
                    if let Some((_, pending_signature)) = pending_thinking.get_mut(&delta.index) {
                        pending_signature
                            .get_or_insert_with(String::new)
                            .push_str(&signature);
                    }
                }
            },
            StreamEvent::ContentBlockStop(stop) => {
                if let Some((thinking, signature)) = pending_thinking.remove(&stop.index) {
                    events.push(AssistantEvent::Thinking {
                        thinking,
                        signature,
                    });
                }
                if let Some((id, name, input)) = pending_tools.remove(&stop.index) {
                    events.push(AssistantEvent::ToolUse { id, name, input });
                }
            }
            StreamEvent::MessageDelta(delta) => {
                events.push(AssistantEvent::Usage(delta.usage.token_usage()));
            }
            StreamEvent::MessageStop(_) => {
                saw_stop = true;
                events.push(AssistantEvent::MessageStop);
            }
        }
    }

    if *cancel_rx.borrow() {
        return Err(api::ApiError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Streaming cancelled",
        )));
    }

    if !saw_stop
        && events.iter().any(|event| {
            matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                || matches!(event, AssistantEvent::ToolUse { .. })
        })
    {
        events.push(AssistantEvent::MessageStop);
    }

    Ok(events)
}

/// TUI-integrated ApiClient implementation driving Crudo's ProviderClient.
struct TuiApiClient {
    client: ProviderClient,
    model: String,
    tool_registry: GlobalToolRegistry,
    tx: mpsc::Sender<(usize, AgentEvent)>,
    run_id: usize,
    cancel_rx: watch::Receiver<bool>,
    handle: tokio::runtime::Handle,
}

impl ApiClient for TuiApiClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if *self.cancel_rx.borrow() {
            return Err(RuntimeError::new("Turn cancelled before API streaming"));
        }
        let tools: Vec<ToolDefinition> =
            filter_tool_definitions(self.tool_registry.definitions(None));
        let messages = convert_messages(&request.messages);
        let system =
            (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n"));
        let tool_choice = (!tools.is_empty()).then_some(ToolChoice::Auto);

        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages,
            system,
            tools: (!tools.is_empty()).then_some(tools),
            tool_choice,
            stream: true,
            ..Default::default()
        };

        let client = &self.client;
        let tx = &self.tx;
        let run_id = self.run_id;
        let cancel_rx = self.cancel_rx.clone();

        self.handle.block_on(async {
            stream_with_provider(client, &message_request, tx, run_id, cancel_rx)
                .await
                .map_err(|e| RuntimeError::new(format!("API streaming request failed: {e}")))
        })
    }
}

/// Tool executor bridging Crudo's GlobalToolRegistry to the TUI's AgentEvent activity system.
struct TuiToolExecutor {
    registry: GlobalToolRegistry,
    tx: mpsc::Sender<(usize, AgentEvent)>,
    run_id: usize,
    next_tool_id: usize,
    cancel_rx: watch::Receiver<bool>,
}

impl ToolExecutor for TuiToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if *self.cancel_rx.borrow() {
            return Err(ToolError::new("Tool execution cancelled"));
        }

        self.next_tool_id += 1;
        let tool_id = self.next_tool_id;

        let input_value: serde_json::Value =
            serde_json::from_str(input).unwrap_or_else(|_| serde_json::json!({ "input": input }));

        let summary = summarize_tool_input(tool_name, &input_value);

        // 1. Emit AgentEvent::ToolStarted
        let _ = self.tx.blocking_send((
            self.run_id,
            AgentEvent::ToolStarted {
                tool: tool_name.to_string(),
                summary,
            },
        ));

        let start_time = std::time::Instant::now();

        // 2. Execute through GlobalToolRegistry
        let execution_result = self.registry.execute(tool_name, &input_value);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        if *self.cancel_rx.borrow() {
            let _ = self.tx.blocking_send((
                self.run_id,
                AgentEvent::ToolFinished {
                    id: tool_id,
                    duration_ms,
                },
            ));
            return Err(ToolError::new("Tool execution cancelled"));
        }

        // 3. Emit AgentEvent::ToolOutput and AgentEvent::ToolFinished
        match execution_result {
            Ok(output) => {
                let _ = self.tx.blocking_send((
                    self.run_id,
                    AgentEvent::ToolOutput {
                        id: tool_id,
                        output: output.clone(),
                    },
                ));
                let _ = self.tx.blocking_send((
                    self.run_id,
                    AgentEvent::ToolFinished {
                        id: tool_id,
                        duration_ms,
                    },
                ));
                Ok(output)
            }
            Err(err) => {
                let _ = self.tx.blocking_send((
                    self.run_id,
                    AgentEvent::ToolOutput {
                        id: tool_id,
                        output: format!("Error: {err}"),
                    },
                ));
                let _ = self.tx.blocking_send((
                    self.run_id,
                    AgentEvent::ToolFinished {
                        id: tool_id,
                        duration_ms,
                    },
                ));
                Err(ToolError::new(err))
            }
        }
    }
}

/// Permission prompter bridging Crudo's permission evaluation to the TUI event loop.
/// Non-blocking to the TUI: emits PermissionRequested and waits with a timeout loop
/// checking cancellation so that user dismissal/cancellation unblocks immediately.
pub struct TuiPermissionPrompter {
    tx: mpsc::Sender<(usize, AgentEvent)>,
    run_id: usize,
    next_request_id: usize,
    cancel_rx: watch::Receiver<bool>,
}

impl PermissionPrompter for TuiPermissionPrompter {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
        if *self.cancel_rx.borrow() {
            return PermissionPromptDecision::Deny {
                reason: "Agent turn was cancelled".to_string(),
            };
        }

        self.next_request_id += 1;
        let request_id = self.next_request_id;

        let input_value: serde_json::Value = serde_json::from_str(&request.input)
            .unwrap_or_else(|_| serde_json::json!({ "input": &request.input }));
        let summary = summarize_tool_input(&request.tool_name, &input_value);

        let (response_tx, response_rx) = std::sync::mpsc::sync_channel(1);
        let responder = super::events::PermissionResponder::new(response_tx);

        let _ = self.tx.blocking_send((
            self.run_id,
            AgentEvent::PermissionRequested {
                id: request_id,
                tool_name: request.tool_name.clone(),
                summary,
                current_mode: request.current_mode.as_str().to_string(),
                reason: request.reason.clone(),
                responder,
            },
        ));

        let decision = loop {
            if *self.cancel_rx.borrow() {
                break PermissionPromptDecision::Deny {
                    reason: "Permission request was cancelled".to_string(),
                };
            }
            match response_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(decision) => break decision,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break PermissionPromptDecision::Deny {
                        reason: "Permission request was dismissed or cancelled".to_string(),
                    };
                }
            }
        };

        let _ = self.tx.blocking_send((
            self.run_id,
            AgentEvent::PermissionResolved {
                id: request_id,
                allowed: matches!(decision, PermissionPromptDecision::Allow),
            },
        ));

        decision
    }
}

/// Sanitizes and simplifies error messages to ensure no credentials, headers,
/// or sensitive variables are exposed, while providing concise, actionable feedback.
pub fn sanitize_error_message(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("connection refused") || lower.contains("failed to connect") {
        return "Connection refused: Unable to reach AI provider API endpoint.".to_string();
    }
    if lower.contains("connection reset") {
        return "Connection reset by peer during request.".to_string();
    }
    if lower.contains("unexpected eof") || lower.contains("premature eof") {
        return "Connection closed prematurely by AI provider.".to_string();
    }
    if lower.contains("timed out") || lower.contains("deadline has elapsed") {
        return "Request to AI provider timed out.".to_string();
    }

    let mut words = Vec::new();
    let mut prev_was_bearer = false;

    for word in err.split_whitespace() {
        if prev_was_bearer {
            words.push("[REDACTED]");
            prev_was_bearer = false;
        } else if word.eq_ignore_ascii_case("bearer") {
            words.push(word);
            prev_was_bearer = true;
        } else if word.starts_with("sk-") || word.starts_with("ant-") {
            words.push("[REDACTED_API_KEY]");
        } else {
            words.push(word);
        }
    }

    words.join(" ")
}

/// Real Agent Adapter connecting the TUI to Crudo's runtime, API client, and tools.
///
/// Flow:
/// TUI -> RealAgentAdapter -> ConversationRuntime -> ToolExecutor -> GlobalToolRegistry -> Actual Tools
#[allow(dead_code)]
pub struct RealAgentAdapter {
    model: Arc<Mutex<Option<String>>>,
    provider_client: Option<ProviderClient>,
    tool_registry: Option<GlobalToolRegistry>,
    permission_mode: Arc<Mutex<Option<PermissionMode>>>,
    session: Arc<Mutex<Session>>,
    timeout: Option<std::time::Duration>,
}

unsafe impl Send for RealAgentAdapter {}
unsafe impl Sync for RealAgentAdapter {}

#[allow(dead_code)]
impl RealAgentAdapter {
    pub fn new() -> Self {
        Self {
            model: Arc::new(Mutex::new(None)),
            provider_client: None,
            tool_registry: None,
            permission_mode: Arc::new(Mutex::new(None)),
            session: Arc::new(Mutex::new(Session::new())),
            timeout: None,
        }
    }

    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: Arc::new(Mutex::new(Some(model.into()))),
            provider_client: None,
            tool_registry: None,
            permission_mode: Arc::new(Mutex::new(None)),
            session: Arc::new(Mutex::new(Session::new())),
            timeout: None,
        }
    }

    pub fn with_client(client: ProviderClient) -> Self {
        Self {
            model: Arc::new(Mutex::new(None)),
            provider_client: Some(client),
            tool_registry: None,
            permission_mode: Arc::new(Mutex::new(None)),
            session: Arc::new(Mutex::new(Session::new())),
            timeout: None,
        }
    }

    pub fn with_components(
        provider_client: Option<ProviderClient>,
        tool_registry: Option<GlobalToolRegistry>,
    ) -> Self {
        Self {
            model: Arc::new(Mutex::new(None)),
            provider_client,
            tool_registry,
            permission_mode: Arc::new(Mutex::new(None)),
            session: Arc::new(Mutex::new(Session::new())),
            timeout: None,
        }
    }

    pub fn with_tool_registry(mut self, registry: GlobalToolRegistry) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = Arc::new(Mutex::new(Some(mode)));
        self
    }

    pub fn with_session(mut self, session: Session) -> Self {
        self.session = Arc::new(Mutex::new(session));
        self
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn session(&self) -> Arc<Mutex<Session>> {
        self.session.clone()
    }

    pub fn set_session(&self, session: Session) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }

    pub fn get_session(&self) -> Session {
        self.session
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| Session::new())
    }

    pub fn reset_session(&self) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = Session::new();
        }
    }

    pub fn model(&self) -> Option<String> {
        self.model.lock().ok().and_then(|g| g.clone())
    }

    pub fn set_model(&self, model: impl Into<String>) {
        if let Ok(mut guard) = self.model.lock() {
            *guard = Some(model.into());
        }
    }

    pub fn permission_mode(&self) -> Option<PermissionMode> {
        self.permission_mode.lock().ok().and_then(|g| *g)
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = Some(mode);
        }
    }
}

impl Default for RealAgentAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for RealAgentAdapter {
    fn name(&self) -> &'static str {
        "RealAgentAdapter"
    }

    fn set_session(&self, session: Session) {
        self.set_session(session);
    }

    fn get_session(&self) -> Option<Session> {
        Some(self.get_session())
    }

    fn model(&self) -> Option<String> {
        self.model()
    }

    fn set_model(&self, model: String) {
        self.set_model(model);
    }

    fn permission_mode(&self) -> Option<PermissionMode> {
        self.permission_mode()
    }

    fn set_permission_mode(&self, mode: PermissionMode) {
        self.set_permission_mode(mode);
    }

    fn execute(
        &self,
        prompt: String,
        tx: mpsc::Sender<(usize, AgentEvent)>,
        mut cancel_rx: watch::Receiver<bool>,
        run_id: usize,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let model_override = self.model();
        let preconfigured_client = self.provider_client.clone();
        let tool_registry = self
            .tool_registry
            .clone()
            .unwrap_or_else(GlobalToolRegistry::builtin);
        let permission_mode_override = self.permission_mode();
        let session_arc = self.session.clone();
        let turn_timeout = self.timeout.or_else(|| {
            std::env::var("CRUDO_TURN_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(std::time::Duration::from_secs)
        });

        let retain_cancelled_user_message = |session: &Arc<Mutex<Session>>, prompt: &str| {
            if let Ok(mut guard) = session.lock() {
                let _ = guard.push_user_text(prompt);
                let _ =
                    guard.push_message(ConversationMessage::assistant(vec![ContentBlock::Text {
                        text: "[Cancelled by user]".to_string(),
                    }]));
            }
        };

        Box::pin(async move {
            if *cancel_rx.borrow() {
                retain_cancelled_user_message(&session_arc, &prompt);
                let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                return;
            }

            // 1. Send Started before processing turn
            let _ = tx.send((run_id, AgentEvent::Started)).await;

            if *cancel_rx.borrow() {
                retain_cancelled_user_message(&session_arc, &prompt);
                let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                return;
            }

            // 2. Resolve model name
            let raw_model = model_override.unwrap_or_else(resolve_configured_model);
            let canonical_model = resolve_model_alias(&raw_model);

            // 3. Resolve ProviderClient
            let client = match preconfigured_client {
                Some(c) => c,
                None => match ProviderClient::from_model(&canonical_model) {
                    Ok(c) => c,
                    Err(err) => {
                        let clean_err = sanitize_error_message(&format!(
                            "Failed to initialize provider client: {err}"
                        ));
                        let _ = tx.send((run_id, AgentEvent::Error(clean_err))).await;
                        return;
                    }
                },
            };

            if *cancel_rx.borrow() {
                retain_cancelled_user_message(&session_arc, &prompt);
                let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                return;
            }

            let handle = tokio::runtime::Handle::current();
            let tx_clone = tx.clone();
            let cancel_rx_worker = cancel_rx.clone();

            let current_session = session_arc
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| Session::new());

            let prompt_for_worker = prompt.clone();

            // 4. Drive ConversationRuntime inside blocking threadpool to keep TUI event loop non-blocking
            let mut blocking_task = tokio::task::spawn_blocking(move || {
                let api_client = TuiApiClient {
                    client,
                    model: canonical_model,
                    tool_registry: tool_registry.clone(),
                    tx: tx_clone.clone(),
                    run_id,
                    cancel_rx: cancel_rx_worker.clone(),
                    handle,
                };
                let tool_executor = TuiToolExecutor {
                    registry: tool_registry.clone(),
                    tx: tx_clone.clone(),
                    run_id,
                    next_tool_id: 0,
                    cancel_rx: cancel_rx_worker.clone(),
                };
                let active_mode = permission_mode_override.unwrap_or_else(|| {
                    if let Ok(val) = std::env::var("CRUDO_PERMISSION_MODE") {
                        match val.trim().to_lowercase().as_str() {
                            "read-only" => PermissionMode::ReadOnly,
                            "workspace-write" => PermissionMode::WorkspaceWrite,
                            "danger-full-access" => PermissionMode::DangerFullAccess,
                            "allow" => PermissionMode::Allow,
                            _ => PermissionMode::WorkspaceWrite,
                        }
                    } else {
                        PermissionMode::WorkspaceWrite
                    }
                });

                let mut policy = PermissionPolicy::new(active_mode);
                if let Ok(specs) = tool_registry.permission_specs(None) {
                    for (name, req_mode) in specs {
                        policy = policy.with_tool_requirement(name, req_mode);
                    }
                }

                let mut prompter = TuiPermissionPrompter {
                    tx: tx_clone,
                    run_id,
                    next_request_id: 0,
                    cancel_rx: cancel_rx_worker,
                };

                let mut runtime = ConversationRuntime::new(
                    current_session,
                    api_client,
                    tool_executor,
                    policy,
                    Vec::new(),
                );

                let result = runtime.run_turn(prompt_for_worker, Some(&mut prompter));
                (result, runtime.into_session())
            });

            let timeout_fut = async {
                if let Some(duration) = turn_timeout {
                    tokio::time::sleep(duration).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };

            // 5. Handle turn outcome, cancellation, or timeout and notify TUI
            tokio::select! {
                task_res = &mut blocking_task => {
                    if *cancel_rx.borrow() {
                        retain_cancelled_user_message(&session_arc, &prompt);
                        let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                    } else {
                        match task_res {
                            Ok((Ok(_turn_summary), final_session)) => {
                                if let Ok(mut guard) = session_arc.lock() {
                                    *guard = final_session;
                                }
                                let _ = tx.send((run_id, AgentEvent::Completed)).await;
                            }
                            Ok((Err(runtime_err), _)) => {
                                if *cancel_rx.borrow() {
                                    retain_cancelled_user_message(&session_arc, &prompt);
                                    let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                                } else {
                                    let clean_err = sanitize_error_message(&runtime_err.to_string());
                                    let _ = tx
                                        .send((run_id, AgentEvent::Error(clean_err)))
                                        .await;
                                }
                            }
                            Err(join_err) => {
                                if *cancel_rx.borrow() {
                                    retain_cancelled_user_message(&session_arc, &prompt);
                                    let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                                } else {
                                    let clean_err = sanitize_error_message(&format!("Runtime execution panicked: {join_err}"));
                                    let _ = tx
                                        .send((
                                            run_id,
                                            AgentEvent::Error(clean_err),
                                        ))
                                        .await;
                                }
                            }
                        }
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        retain_cancelled_user_message(&session_arc, &prompt);
                        let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                    }
                }
                _ = timeout_fut => {
                    blocking_task.abort();
                    let timeout_msg = if let Some(d) = turn_timeout {
                        format!("Agent operation timed out after {:.1}s", d.as_secs_f64())
                    } else {
                        "Agent operation timed out".to_string()
                    };
                    let _ = tx.send((run_id, AgentEvent::Error(timeout_msg))).await;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::AnthropicClient;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_real_agent_adapter_construction() {
        let adapter = RealAgentAdapter::new();
        assert_eq!(adapter.name(), "RealAgentAdapter");
        assert!(adapter.model.lock().unwrap().is_none());
        assert!(adapter.provider_client.is_none());
        assert!(adapter.tool_registry.is_none());
    }

    #[test]
    fn test_real_agent_adapter_with_components() {
        let registry = GlobalToolRegistry::builtin();
        let adapter = RealAgentAdapter::with_components(None, Some(registry));
        assert!(adapter.tool_registry.is_some());
    }

    #[tokio::test]
    async fn test_real_agent_adapter_cancellation() {
        let adapter = RealAgentAdapter::new();
        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(true); // Already cancelled
        drop(cancel_tx);

        adapter
            .execute("hello".to_string(), tx, cancel_rx, 99)
            .await;

        let event = rx.try_recv().expect("event should be received");
        assert_eq!(event.0, 99);
        assert!(matches!(event.1, AgentEvent::Cancelled));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_missing_credentials_emits_error() {
        // When constructed without a valid client or API keys, should emit Started then Error
        let adapter = RealAgentAdapter::with_model("claude-sonnet-4-6");
        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter.execute("hello".to_string(), tx, cancel_rx, 1).await;

        let ev1 = rx.recv().await.expect("ev1");
        assert_eq!(ev1.0, 1);
        assert!(matches!(ev1.1, AgentEvent::Started));

        let ev2 = rx.recv().await.expect("ev2");
        assert_eq!(ev2.0, 1);
        match ev2.1 {
            AgentEvent::Error(_) | AgentEvent::TextChunk(_) => {}
            other => panic!("Expected Error or TextChunk, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_real_agent_adapter_streaming_flow_with_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                // Read HTTP request until headers are finished
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"from CRUDO!\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter.execute("Hi".to_string(), tx, cancel_rx, 7).await;

        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 7);
        assert!(matches!(ev1.1, AgentEvent::Started));

        let ev2 = rx.recv().await.expect("TextChunk 1");
        assert_eq!(ev2.0, 7);
        if let AgentEvent::TextChunk(text) = ev2.1 {
            assert_eq!(text, "Hello ");
        } else {
            panic!("Expected TextChunk, got: {:?}", ev2.1);
        }

        let ev3 = rx.recv().await.expect("TextChunk 2");
        assert_eq!(ev3.0, 7);
        if let AgentEvent::TextChunk(text) = ev3.1 {
            assert_eq!(text, "from CRUDO!");
        } else {
            panic!("Expected TextChunk, got: {:?}", ev3.1);
        }

        let ev4 = rx.recv().await.expect("Completed event");
        assert_eq!(ev4.0, 7);
        assert!(matches!(ev4.1, AgentEvent::Completed));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_tool_execution_flow() {
        // Test end-to-end tool execution through ConversationRuntime and GlobalToolRegistry
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else if turn == 0 {
                    turn += 1;
                    // First turn: model requests a tool call to read_file Cargo.toml
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_read_1\",\"name\":\"read_file\",\"input\":{}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"Cargo.toml\\\"}\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    // Second turn: model receives tool result and responds with text
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Read Cargo.toml successfully!\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Read Cargo.toml".to_string(), tx, cancel_rx, 42)
            .await;

        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 42);
        assert!(matches!(ev1.1, AgentEvent::Started));

        // ToolStarted event
        let ev2 = rx.recv().await.expect("ToolStarted event");
        assert_eq!(ev2.0, 42);
        if let AgentEvent::ToolStarted { tool, summary } = ev2.1 {
            assert_eq!(tool, "read_file");
            assert_eq!(summary, "Cargo.toml");
        } else {
            panic!("Expected ToolStarted, got: {:?}", ev2.1);
        }

        // ToolOutput event
        let ev3 = rx.recv().await.expect("ToolOutput event");
        assert_eq!(ev3.0, 42);
        if let AgentEvent::ToolOutput { id, output } = ev3.1 {
            assert_eq!(id, 1);
            assert!(output.contains("crudo-tui"));
        } else {
            panic!("Expected ToolOutput, got: {:?}", ev3.1);
        }

        // ToolFinished event
        let ev4 = rx.recv().await.expect("ToolFinished event");
        assert_eq!(ev4.0, 42);
        if let AgentEvent::ToolFinished { id, .. } = ev4.1 {
            assert_eq!(id, 1);
        } else {
            panic!("Expected ToolFinished, got: {:?}", ev4.1);
        }

        // Second turn: TextChunk
        let ev5 = rx.recv().await.expect("TextChunk event");
        assert_eq!(ev5.0, 42);
        if let AgentEvent::TextChunk(text) = ev5.1 {
            assert_eq!(text, "Read Cargo.toml successfully!");
        } else {
            panic!("Expected TextChunk, got: {:?}", ev5.1);
        }

        // Completed event
        let ev6 = rx.recv().await.expect("Completed event");
        assert_eq!(ev6.0, 42);
        assert!(matches!(ev6.1, AgentEvent::Completed));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_permission_allow_flow() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&buffer);
                if req_str.contains("POST /v1/messages/count_tokens") {
                    let body = "{\"input_tokens\":5}";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn += 1;
                if turn == 1 {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_bash_1\",\"name\":\"bash\",\"input\":{}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"cargo --version\\\"}\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Permission was allowed and tool executed.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        tokio::spawn(async move {
            adapter
                .execute("Run cargo --version".to_string(), tx, cancel_rx, 77)
                .await;
        });

        // 1. Started
        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 77);
        assert!(matches!(ev1.1, AgentEvent::Started));

        // 2. PermissionRequested
        let ev2 = rx.recv().await.expect("PermissionRequested event");
        assert_eq!(ev2.0, 77);
        if let AgentEvent::PermissionRequested {
            id,
            tool_name,
            summary,
            responder,
            ..
        } = ev2.1
        {
            assert_eq!(id, 1);
            assert_eq!(tool_name, "bash");
            assert_eq!(summary, "cargo --version");
            // Approve permission
            responder.respond(runtime::PermissionPromptDecision::Allow);
        } else {
            panic!("Expected PermissionRequested, got: {:?}", ev2.1);
        }

        // 3. PermissionResolved
        let ev3 = rx.recv().await.expect("PermissionResolved event");
        assert_eq!(ev3.0, 77);
        if let AgentEvent::PermissionResolved { id, allowed } = ev3.1 {
            assert_eq!(id, 1);
            assert!(allowed);
        } else {
            panic!("Expected PermissionResolved, got: {:?}", ev3.1);
        }

        // 4. Tool execution follows: ToolStarted
        let ev4 = rx.recv().await.expect("ToolStarted event");
        assert_eq!(ev4.0, 77);
        assert!(matches!(ev4.1, AgentEvent::ToolStarted { .. }));

        // 5. ToolOutput
        let ev5 = rx.recv().await.expect("ToolOutput event");
        assert_eq!(ev5.0, 77);
        assert!(matches!(ev5.1, AgentEvent::ToolOutput { .. }));

        // 6. ToolFinished
        let ev6 = rx.recv().await.expect("ToolFinished event");
        assert_eq!(ev6.0, 77);
        assert!(matches!(ev6.1, AgentEvent::ToolFinished { .. }));

        // 7. TextChunk from model
        let ev7 = rx.recv().await.expect("TextChunk event");
        assert_eq!(ev7.0, 77);
        if let AgentEvent::TextChunk(text) = ev7.1 {
            assert!(text.contains("Permission was allowed"));
        } else {
            panic!("Expected TextChunk, got: {:?}", ev7.1);
        }

        // 8. Completed
        let ev8 = rx.recv().await.expect("Completed event");
        assert_eq!(ev8.0, 77);
        assert!(matches!(ev8.1, AgentEvent::Completed));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_permission_deny_flow() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&buffer);
                if req_str.contains("POST /v1/messages/count_tokens") {
                    let body = "{\"input_tokens\":5}";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn += 1;
                if turn == 1 {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_bash_2\",\"name\":\"bash\",\"input\":{}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"rm -rf /\\\"}\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"I understand you denied access.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        tokio::spawn(async move {
            adapter
                .execute("Run dangerous command".to_string(), tx, cancel_rx, 88)
                .await;
        });

        // 1. Started
        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 88);
        assert!(matches!(ev1.1, AgentEvent::Started));

        // 2. PermissionRequested
        let ev2 = rx.recv().await.expect("PermissionRequested event");
        assert_eq!(ev2.0, 88);
        if let AgentEvent::PermissionRequested {
            id,
            tool_name,
            summary,
            responder,
            ..
        } = ev2.1
        {
            assert_eq!(id, 1);
            assert_eq!(tool_name, "bash");
            assert_eq!(summary, "rm -rf /");
            // Deny permission
            responder.respond(runtime::PermissionPromptDecision::Deny {
                reason: "Denied by user test".to_string(),
            });
        } else {
            panic!("Expected PermissionRequested, got: {:?}", ev2.1);
        }

        // 3. PermissionResolved
        let ev3 = rx.recv().await.expect("PermissionResolved event");
        assert_eq!(ev3.0, 88);
        if let AgentEvent::PermissionResolved { id, allowed } = ev3.1 {
            assert_eq!(id, 1);
            assert!(!allowed);
        } else {
            panic!("Expected PermissionResolved, got: {:?}", ev3.1);
        }

        // 4. Note: Tool does NOT run! No ToolStarted/ToolFinished!
        // Instead, the denial was passed to ConversationRuntime, and model responds with TextChunk:
        let ev4 = rx.recv().await.expect("TextChunk event");
        assert_eq!(ev4.0, 88);
        if let AgentEvent::TextChunk(text) = ev4.1 {
            assert!(text.contains("denied access"));
        } else {
            panic!("Expected TextChunk, got: {:?}", ev4.1);
        }

        // 5. Completed
        let ev5 = rx.recv().await.expect("Completed event");
        assert_eq!(ev5.0, 88);
        assert!(matches!(ev5.1, AgentEvent::Completed));
    }

    #[tokio::test]
    async fn test_cancellation_during_streaming_stops_and_emits_cancelled() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&buffer);
                if req_str.contains("POST /v1/messages/count_tokens") {
                    let body = "{\"input_tokens\":5}";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                // Send first chunk and keep connection open until cancelled
                let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
                let _ = socket.write_all(headers.as_bytes()).await;

                let part1 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_slow\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n";
                let chunk1 = format!("{:x}\r\n{}\r\n", part1.len(), part1);
                let _ = socket.write_all(chunk1.as_bytes()).await;

                let part2 = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n";
                let chunk2 = format!("{:x}\r\n{}\r\n", part2.len(), part2);
                let _ = socket.write_all(chunk2.as_bytes()).await;

                let part3 = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}\n\n";
                let chunk3 = format!("{:x}\r\n{}\r\n", part3.len(), part3);
                let _ = socket.write_all(chunk3.as_bytes()).await;

                // Now pause without sending more data until the client disconnects/cancels
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let exec_handle = tokio::spawn(async move {
            adapter
                .execute("Test slow streaming".to_string(), tx, cancel_rx, 42)
                .await;
        });

        // 1. Started
        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 42);
        assert!(matches!(ev1.1, AgentEvent::Started));

        // 2. First text chunk
        let ev2 = rx.recv().await.expect("First TextChunk event");
        assert_eq!(ev2.0, 42);
        if let AgentEvent::TextChunk(text) = ev2.1 {
            assert_eq!(text, "Hello ");
        } else {
            panic!("Expected TextChunk, got: {:?}", ev2.1);
        }

        // 3. User triggers cancellation
        cancel_tx.send(true).unwrap();

        // 4. Next event must be Cancelled
        let ev3 = rx.recv().await.expect("Cancelled event");
        assert_eq!(ev3.0, 42);
        assert!(matches!(ev3.1, AgentEvent::Cancelled));

        // Wait for execution handle to complete cleanly
        tokio::time::timeout(std::time::Duration::from_secs(1), exec_handle)
            .await
            .expect("Execution task should finish promptly")
            .expect("Task should not panic");
    }

    #[tokio::test]
    async fn test_cancellation_while_permission_is_pending_unblocks_cleanly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&buffer);
                if req_str.contains("POST /v1/messages/count_tokens") {
                    let body = "{\"input_tokens\":5}";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                // Request bash tool call
                let sse_body = concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_perm_cancel\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                    "event: content_block_start\n",
                    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_bash_cancel\",\"name\":\"bash\",\"input\":{}}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"cargo clean\\\"}\"}}\n\n",
                    "event: content_block_stop\n",
                    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n",
                    "data: [DONE]\n\n"
                );

                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let exec_handle = tokio::spawn(async move {
            adapter
                .execute("Run dangerous command".to_string(), tx, cancel_rx, 77)
                .await;
        });

        // 1. Started
        let ev1 = rx.recv().await.expect("Started event");
        assert_eq!(ev1.0, 77);
        assert!(matches!(ev1.1, AgentEvent::Started));

        // 2. PermissionRequested
        let ev2 = rx.recv().await.expect("PermissionRequested event");
        assert_eq!(ev2.0, 77);
        assert!(matches!(ev2.1, AgentEvent::PermissionRequested { .. }));

        // 3. User cancels instead of responding to permission modal
        cancel_tx.send(true).unwrap();

        // 4. Cancelled event must be received
        let mut got_cancelled = false;
        let mut _got_permission_resolved = false;
        while let Ok(ev) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            let Some((run_id, agent_event)) = ev else {
                break;
            };
            assert_eq!(run_id, 77);
            match agent_event {
                AgentEvent::Cancelled => {
                    got_cancelled = true;
                    break;
                }
                AgentEvent::PermissionResolved { allowed, .. } => {
                    assert!(!allowed);
                    _got_permission_resolved = true;
                }
                _ => {}
            }
        }
        assert!(got_cancelled, "Agent must emit Cancelled event");

        // Wait for execution task to terminate cleanly
        tokio::time::timeout(std::time::Duration::from_secs(1), exec_handle)
            .await
            .expect("Execution task should unblock promptly on cancel")
            .expect("Task should not panic");
    }

    #[test]
    fn test_cancellation_does_not_leave_pending_permission_stuck_in_app() {
        use crate::app::{AgentState, App, PendingPermission, PermissionChoice};
        use crate::command::AppCommand;

        let mut app = App::new();
        let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel(1);
        let responder = super::super::events::PermissionResponder::new(resp_tx);

        app.set_agent_state(AgentState::Thinking);
        app.pending_permission = Some(PendingPermission {
            id: 1,
            tool_name: "bash".to_string(),
            summary: "rm -rf /".to_string(),
            current_mode: "workspace-write".to_string(),
            reason: None,
            choice: PermissionChoice::Allow,
            responder,
        });

        // User issues CancelAgent command
        app.handle_command(AppCommand::CancelAgent);

        // pending_permission must be cleared
        assert!(app.pending_permission.is_none());
        assert_eq!(app.agent_state, AgentState::Idle);

        // Responder channel must receive Deny
        let decision = resp_rx
            .try_recv()
            .expect("responder should receive decision");
        assert!(matches!(
            decision,
            runtime::PermissionPromptDecision::Deny { .. }
        ));
    }

    #[tokio::test]
    async fn test_cancellation_followed_by_new_prompt_works_normally() {
        use crate::agent::controller::AgentController;

        let mut controller = AgentController::with_agent(RealAgentAdapter::new());
        let (tx, mut rx) = mpsc::channel(16);

        // 1. First prompt
        controller.submit_prompt("First prompt".to_string(), tx.clone());
        assert!(controller.is_running());
        assert_eq!(controller.current_run_id(), 1);

        // Cancel first run
        controller.cancel();
        assert!(!controller.is_running());

        // Wait for run 1 to emit its Cancelled event
        let ev1 = rx.recv().await.expect("Cancelled event from run 1");
        assert_eq!(ev1.0, 1);
        assert!(matches!(ev1.1, AgentEvent::Cancelled));

        // 2. Second prompt after cancellation
        controller.submit_prompt("Second prompt".to_string(), tx.clone());
        assert!(controller.is_running());
        assert_eq!(controller.current_run_id(), 2);

        // Wait for event from second run
        let ev2 = rx.recv().await.expect("Event from run 2");
        assert_eq!(ev2.0, 2);
        assert!(matches!(ev2.1, AgentEvent::Started));
    }

    #[test]
    fn test_duplicate_cancellation_does_not_panic() {
        use crate::agent::controller::AgentController;
        use crate::app::{AgentState, App};
        use crate::command::AppCommand;

        let mut controller = AgentController::with_agent(RealAgentAdapter::new());
        controller.cancel();
        controller.cancel();
        controller.cancel();
        assert!(!controller.is_running());

        let mut app = App::new();
        app.handle_command(AppCommand::CancelAgent);
        app.handle_command(AppCommand::CancelAgent);
        app.handle_command(AppCommand::CancelAgent);
        assert_eq!(app.agent_state, AgentState::Idle);
    }

    #[test]
    fn test_real_agent_adapter_session_initialization_and_reset() {
        let adapter = RealAgentAdapter::new();
        {
            let guard = adapter.session();
            let mut session = guard.lock().unwrap();
            assert!(session.messages.is_empty());
            let _ = session.push_user_text("test message");
            assert_eq!(session.messages.len(), 1);
        }
        adapter.reset_session();
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert!(session.messages.is_empty());
        }
    }

    #[tokio::test]
    async fn test_real_agent_adapter_thinking_event_normalization() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    let sse_body = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"I am thinking\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Final answer\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );

                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        sse_body.len(),
                        sse_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Reason about this".to_string(), tx, cancel_rx, 42)
            .await;

        let mut events = Vec::new();
        while let Some((_, ev)) = rx.recv().await {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Thinking)));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextChunk(t) if t == "Final answer")));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Completed)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_qwen3_split_think_tags_separated_from_text_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let sse_body = concat!(
                    "data: {\"id\":\"1\",\"model\":\"qwen3:4b\",\"choices\":[{\"delta\":{\"content\":\"<th\"}}]}\n\n",
                    "data: {\"id\":\"2\",\"choices\":[{\"delta\":{\"content\":\"ink>internal reasoning...</thi\"}}]}\n\n",
                    "data: {\"id\":\"3\",\"choices\":[{\"delta\":{\"content\":\"nk>Rust is fast.\"}}]}\n\n",
                    "data: {\"id\":\"4\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                    "data: [DONE]\n\n"
                );

                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let client = api::OpenAiCompatClient::new("test-key", api::OpenAiCompatConfig::openai())
            .with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::OpenAi(client);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Explain Rust".to_string(), tx, cancel_rx, 100)
            .await;

        let mut events = Vec::new();
        while let Some((_, ev)) = rx.recv().await {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Thinking)));

        let mut text_chunks = Vec::new();
        for ev in &events {
            if let AgentEvent::TextChunk(c) = ev {
                text_chunks.push(c.clone());
            }
        }

        let full_text = text_chunks.join("");
        assert_eq!(full_text, "Rust is fast.");

        // Critical: Reasoning text and tags must NEVER appear in TextChunk
        for chunk in &text_chunks {
            assert!(!chunk.contains("<think>"));
            assert!(!chunk.contains("</think>"));
            assert!(!chunk.contains("<th"));
            assert!(!chunk.contains("ink>"));
            assert!(!chunk.contains("</thi"));
            assert!(!chunk.contains("nk>"));
            assert!(!chunk.contains("internal reasoning"));
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Completed)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_ollama_reasoning_field_separated_from_text_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let sse_body = concat!(
                    "data: {\"id\":\"1\",\"model\":\"qwen3:4b\",\"choices\":[{\"delta\":{\"reasoning\":\"Evaluating question...\"}}]}\n\n",
                    "data: {\"id\":\"2\",\"choices\":[{\"delta\":{\"content\":\"Rust provides memory safety.\"},\"finish_reason\":\"stop\"}]}\n\n",
                    "data: [DONE]\n\n"
                );

                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let client = api::OpenAiCompatClient::new("test-key", api::OpenAiCompatConfig::openai())
            .with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::OpenAi(client);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Explain Rust".to_string(), tx, cancel_rx, 101)
            .await;

        let mut events = Vec::new();
        while let Some((_, ev)) = rx.recv().await {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Thinking)));

        let mut text_chunks = Vec::new();
        for ev in &events {
            if let AgentEvent::TextChunk(c) = ev {
                text_chunks.push(c.clone());
            }
        }

        let full_text = text_chunks.join("");
        assert_eq!(full_text, "Rust provides memory safety.");

        for chunk in &text_chunks {
            assert!(!chunk.contains("Evaluating question"));
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Completed)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_multi_turn_state_preservation() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                let mut content_length = 0;
                let mut header_len = 0;

                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);

                    if header_len == 0 {
                        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_len = pos + 4;
                            let header_str = String::from_utf8_lossy(&buffer[..header_len]);
                            for line in header_str.lines() {
                                if let Some(stripped) = line.strip_prefix("content-length:") {
                                    content_length = stripped.trim().parse::<usize>().unwrap_or(0);
                                } else if let Some(stripped) = line.strip_prefix("Content-Length:")
                                {
                                    content_length = stripped.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }

                    if header_len > 0 && buffer.len() >= header_len + content_length {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else {
                    turn += 1;
                    if turn == 1 {
                        assert!(request_str.contains("Hello turn 1"));

                        let sse_body = concat!(
                            "event: message_start\n",
                            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                            "event: content_block_start\n",
                            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                            "event: content_block_delta\n",
                            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Reply 1\"}}\n\n",
                            "event: content_block_stop\n",
                            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                            "event: message_stop\n",
                            "data: {\"type\":\"message_stop\"}\n\n",
                            "data: [DONE]\n\n"
                        );

                        let resp = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            sse_body.len(),
                            sse_body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        let _ = socket.shutdown().await;
                    } else {
                        // Turn 2: must contain previous turn messages ("Hello turn 1" and "Reply 1") AND current "Hello turn 2"
                        assert!(request_str.contains("Hello turn 1"));
                        assert!(request_str.contains("Reply 1"));
                        assert!(request_str.contains("Hello turn 2"));

                        let sse_body = concat!(
                            "event: message_start\n",
                            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                            "event: content_block_start\n",
                            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                            "event: content_block_delta\n",
                            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Reply 2\"}}\n\n",
                            "event: content_block_stop\n",
                            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                            "event: message_stop\n",
                            "data: {\"type\":\"message_stop\"}\n\n",
                            "data: [DONE]\n\n"
                        );

                        let resp = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            sse_body.len(),
                            sse_body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        let _ = socket.shutdown().await;
                        break;
                    }
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key").with_base_url(format!("http://{addr}"));
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        // Turn 1
        let (tx1, mut rx1) = mpsc::channel(16);
        let (_cancel_tx1, cancel_rx1) = watch::channel(false);
        adapter
            .execute("Hello turn 1".to_string(), tx1, cancel_rx1, 1)
            .await;

        while let Some((_, ev)) = rx1.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                break;
            }
        }

        // Verify session has Turn 1
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 2);
        }

        // Turn 2
        let (tx2, mut rx2) = mpsc::channel(16);
        let (_cancel_tx2, cancel_rx2) = watch::channel(false);
        adapter
            .execute("Hello turn 2".to_string(), tx2, cancel_rx2, 2)
            .await;

        while let Some((_, ev)) = rx2.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                break;
            }
        }

        // Verify session now has all 4 messages
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 4);
        }
    }

    #[tokio::test]
    async fn test_real_agent_adapter_premature_disconnect_handling() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                } else {
                    // Send partial response then close connection abruptly without DONE or message_stop
                    let part = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_eof\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}",
                        part
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Test premature disconnect".to_string(), tx, cancel_rx, 55)
            .await;

        let mut events = Vec::new();
        while let Ok(Some((run_id, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            assert_eq!(run_id, 55);
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::Error(_) | AgentEvent::Completed)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_malformed_event_handling() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request_str = String::from_utf8_lossy(&buffer);
                if request_str.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                } else {
                    let body = "{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"corrupted message payload\"}}";
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Test malformed".to_string(), tx, cancel_rx, 66)
            .await;

        let mut events = Vec::new();
        while let Ok(Some((run_id, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            assert_eq!(run_id, 66);
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Error(_))));
    }

    #[test]
    fn test_error_sanitization_removes_secrets() {
        let raw_bearer = "Authorization failure: Bearer sk-ant-api03-12345secrettoken with error";
        let sanitized = sanitize_error_message(raw_bearer);
        assert!(!sanitized.contains("sk-ant-api03-12345secrettoken"));
        assert!(sanitized.contains("[REDACTED]"));

        let raw_key = "Failed request with sk-proj-supersecretkey1234567890 inside";
        let sanitized_key = sanitize_error_message(raw_key);
        assert!(!sanitized_key.contains("supersecretkey1234567890"));
        assert!(sanitized_key.contains("[REDACTED_API_KEY]"));

        let raw_conn = "error sending request for url: connection refused: tcp connect error";
        let sanitized_conn = sanitize_error_message(raw_conn);
        assert_eq!(
            sanitized_conn,
            "Connection refused: Unable to reach AI provider API endpoint."
        );

        let raw_eof = "unexpected EOF while reading response body";
        let sanitized_eof = sanitize_error_message(raw_eof);
        assert_eq!(
            sanitized_eof,
            "Connection closed prematurely by AI provider."
        );
    }

    #[tokio::test]
    async fn test_real_agent_adapter_streaming_error_after_partial_text() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                } else {
                    use tokio::io::AsyncWriteExt;
                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part1 = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_part\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Partial content before crash\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part1.as_bytes()).await;
                    let _ = socket.flush().await;

                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                    let part2 = concat!(
                        "event: error\n",
                        "data: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"Server crashed mid stream\"}}\n\n"
                    );
                    let _ = socket.write_all(part2.as_bytes()).await;
                    let _ = socket.flush().await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter.execute("Hi".to_string(), tx, cancel_rx, 101).await;

        let mut events = Vec::new();
        while let Ok(Some((run_id, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            assert_eq!(run_id, 101);
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextChunk(t) if t == "Partial content before crash")));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Completed)));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Cancelled)));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Error(_))));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_turn_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider)
            .with_timeout(std::time::Duration::from_millis(50));

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Timeout prompt".to_string(), tx, cancel_rx, 102)
            .await;

        let mut events = Vec::new();
        while let Ok(Some((run_id, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            assert_eq!(run_id, 102);
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Started)));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::Error(msg) if msg.contains("timed out"))));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Completed)));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Cancelled)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_error_does_not_emit_cancelled() {
        let adapter = RealAgentAdapter::with_model("claude-sonnet-4-6");
        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("hello".to_string(), tx, cancel_rx, 103)
            .await;

        let mut events = Vec::new();
        while let Ok(Some((_, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Error(_))));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Cancelled)));
    }

    #[tokio::test]
    async fn test_real_agent_adapter_cancelled_does_not_emit_error() {
        let adapter = RealAgentAdapter::new();
        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(true);
        drop(cancel_tx);

        adapter
            .execute("hello".to_string(), tx, cancel_rx, 104)
            .await;

        let mut events = Vec::new();
        while let Ok(Some((_, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Cancelled)));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error(_))));
    }

    #[tokio::test]
    async fn test_session_safety_on_failed_and_cancelled_turns() {
        let adapter = RealAgentAdapter::new();
        {
            let guard = adapter.session();
            let mut s = guard.lock().unwrap();
            let _ = s.push_user_text("Initial consistent message");
            assert_eq!(s.messages.len(), 1);
        }

        // Run turn that fails
        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        adapter
            .execute("Will fail".to_string(), tx, cancel_rx, 105)
            .await;
        while let Ok(Some(_)) =
            tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await
        {}

        // Session must be unchanged (still 1 message)
        {
            let guard = adapter.session();
            let s = guard.lock().unwrap();
            assert_eq!(s.messages.len(), 1);
            assert!(matches!(
                &s.messages[0].blocks[0],
                ContentBlock::Text { text } if text == "Initial consistent message"
            ));
        }

        // Run turn that is cancelled
        let (tx2, mut rx2) = mpsc::channel(16);
        let (cancel_tx2, cancel_rx2) = watch::channel(true);
        drop(cancel_tx2);
        adapter
            .execute("Will cancel".to_string(), tx2, cancel_rx2, 106)
            .await;
        while let Ok(Some(_)) =
            tokio::time::timeout(std::time::Duration::from_millis(200), rx2.recv()).await
        {}

        // Cancelled turn retains user message and synthetic cancellation marker, while incomplete assistant generation is NOT committed
        {
            let guard = adapter.session();
            let s = guard.lock().unwrap();
            assert_eq!(s.messages.len(), 3);
            assert!(matches!(
                &s.messages[0].blocks[0],
                ContentBlock::Text { text } if text == "Initial consistent message"
            ));
            assert_eq!(s.messages[1].role, MessageRole::User);
            assert!(matches!(
                &s.messages[1].blocks[0],
                ContentBlock::Text { text } if text == "Will cancel"
            ));
            assert_eq!(s.messages[2].role, MessageRole::Assistant);
            assert!(matches!(
                &s.messages[2].blocks[0],
                ContentBlock::Text { text } if text == "[Cancelled by user]"
            ));
        }
    }

    #[test]
    fn test_error_and_cancelled_recovery_to_idle() {
        use crate::app::{AgentState, App};
        use crate::command::AppCommand;

        let mut app = App::new();
        app.handle_agent_event(AgentEvent::Error("Network failure".to_string()));
        assert_eq!(app.agent_state, AgentState::Error);
        assert!(!app.is_agent_active());

        // Escape recovers to Idle
        app.handle_command(AppCommand::Escape);
        assert_eq!(app.agent_state, AgentState::Idle);

        // Submitting new prompt also recovers to Idle
        app.set_agent_state(AgentState::Error);
        app.input_state.set_value("New prompt".to_string());
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.agent_state, AgentState::Idle);

        // Cancelled directly transitions to Idle
        app.set_agent_state(AgentState::Thinking);
        app.handle_agent_event(AgentEvent::Cancelled);
        assert_eq!(app.agent_state, AgentState::Idle);
    }

    #[tokio::test]
    async fn test_stale_events_from_previous_run_are_ignored() {
        use crate::agent::controller::AgentController;
        use crate::app::App;

        let mut app = App::new();
        let mut controller = AgentController::new();
        let (tx, _rx) = mpsc::channel(16);

        // Run 1 starts
        controller.submit_prompt("Run 1".to_string(), tx.clone());
        assert_eq!(controller.current_run_id(), 1);

        // Run 1 cancelled
        controller.cancel();

        // Run 2 starts
        controller.submit_prompt("Run 2".to_string(), tx.clone());
        assert_eq!(controller.current_run_id(), 2);

        // Late stale error from Run 1
        let stale_event = (1, AgentEvent::Error("Late error from run 1".to_string()));
        if stale_event.0 == controller.current_run_id() {
            app.handle_agent_event(stale_event.1);
        }

        // Stale completion from Run 1
        let stale_comp = (1, AgentEvent::Completed);
        if stale_comp.0 == controller.current_run_id() {
            app.handle_agent_event(stale_comp.1);
        }

        // App state must NOT be corrupted by stale run 1 events
        assert_eq!(app.messages.len(), 0);
        assert_eq!(app.agent_state, crate::app::AgentState::Idle);
    }

    #[test]
    fn test_convert_messages_merges_consecutive_user_prompts() {
        let msg1 = ConversationMessage::user_text("what is game");
        let msg2 = ConversationMessage::user_text("continue");
        let converted = convert_messages(&[msg1, msg2]);

        // Consecutive user messages must merge into 1 InputMessage with 2 content blocks
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content.len(), 2);
        assert!(matches!(
            &converted[0].content[0],
            InputContentBlock::Text { text } if text == "what is game"
        ));
        assert!(matches!(
            &converted[0].content[1],
            InputContentBlock::Text { text } if text == "continue"
        ));
    }

    #[tokio::test]
    async fn test_cancellation_preserves_user_message_and_excludes_incomplete_assistant_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                } else {
                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_cancel\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A game is a structured activity\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.flush().await;

                    // Keep socket open long enough for cancellation to trigger
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let adapter_clone = adapter.clone();
        let prompt_task = tokio::spawn(async move {
            adapter_clone
                .execute("what is game".to_string(), tx, cancel_rx, 201)
                .await;
        });

        // Wait until text chunk arrives, then cancel
        let mut got_chunk = false;
        while let Some((run_id, ev)) = rx.recv().await {
            assert_eq!(run_id, 201);
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("A game is") {
                    got_chunk = true;
                    let _ = cancel_tx.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        assert!(
            got_chunk,
            "Should have received partial text chunk before cancellation"
        );

        let _ = prompt_task.await;

        // Verify session retains user input and cancellation marker, but excludes incomplete assistant generation
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert!(matches!(
            &session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "what is game"
        ));
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[1].blocks[0],
            ContentBlock::Text { text } if text == "[Cancelled by user]"
        ));
        assert!(
            !session
                .messages
                .iter()
                .any(|m| m.blocks.iter().any(|b| match b {
                    ContentBlock::Text { text } => text.contains("structured activity"),
                    _ => false,
                })),
            "Incomplete assistant generation must NOT be committed to session"
        );
    }

    #[tokio::test]
    async fn test_cancellation_sequence_what_is_game_cancel_continue() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn_count = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn_count += 1;
                if turn_count == 1 {
                    // Turn 1: "what is rust in single line" -> returns completed answer
                    assert!(req.contains("what is rust in single line"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Rust is a memory-safe language.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 2 {
                    // Turn 2: "what is game" -> streams partial text then cancelled by client
                    assert!(req.contains("what is game"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A game is an activity...\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.flush().await;

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 3 {
                    // Turn 3: "continue" -> MUST see "what is game", "[Cancelled by user]", and "continue", NOT partial "A game is an activity..."
                    assert!(req.contains("what is rust in single line"));
                    assert!(req.contains("Rust is a memory-safe language."));
                    assert!(req.contains("what is game"));
                    assert!(req.contains("[Cancelled by user]"));
                    assert!(req.contains("continue"));
                    assert!(!req.contains("A game is an activity..."));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_3\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A game is an interactive pastime governed by rules.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        // 1. Successful Turn 1
        let (tx1, mut rx1) = mpsc::channel(16);
        let (_cancel_tx1, cancel_rx1) = watch::channel(false);
        adapter
            .execute(
                "what is rust in single line".to_string(),
                tx1,
                cancel_rx1,
                1,
            )
            .await;
        while let Some((_, ev)) = rx1.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                break;
            }
        }

        // 2. Cancelled Turn 2 ("what is game")
        let (tx2, mut rx2) = mpsc::channel(16);
        let (cancel_tx2, cancel_rx2) = watch::channel(false);
        let adapter_clone = adapter.clone();
        let turn2_task = tokio::spawn(async move {
            adapter_clone
                .execute("what is game".to_string(), tx2, cancel_rx2, 2)
                .await;
        });

        while let Some((_, ev)) = rx2.recv().await {
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("A game is") {
                    let _ = cancel_tx2.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        let _ = turn2_task.await;

        // Session after Turn 2 cancellation:
        // Must contain Turn 1 user + assistant, Turn 2 user, and Turn 2 synthetic cancellation marker.
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 4);
            assert_eq!(session.messages[0].role, MessageRole::User);
            assert_eq!(session.messages[1].role, MessageRole::Assistant);
            assert_eq!(session.messages[2].role, MessageRole::User);
            assert!(matches!(
                &session.messages[2].blocks[0],
                ContentBlock::Text { text } if text == "what is game"
            ));
            assert_eq!(session.messages[3].role, MessageRole::Assistant);
            assert!(matches!(
                &session.messages[3].blocks[0],
                ContentBlock::Text { text } if text == "[Cancelled by user]"
            ));
        }

        // 3. Subsequent Turn 3 ("continue")
        let (tx3, mut rx3) = mpsc::channel(16);
        let (_cancel_tx3, cancel_rx3) = watch::channel(false);
        adapter
            .execute("continue".to_string(), tx3, cancel_rx3, 3)
            .await;

        let mut turn3_completed = false;
        while let Some((_, ev)) = rx3.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                turn3_completed = true;
                break;
            }
        }
        assert!(turn3_completed, "Turn 3 should complete successfully");

        // Final session state: 6 messages (Rust User, Rust Assistant, Game User, Cancelled Assistant, Continue User, Game Assistant)
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 6);
            assert_eq!(session.messages[2].role, MessageRole::User);
            assert!(matches!(
                &session.messages[2].blocks[0],
                ContentBlock::Text { text } if text == "what is game"
            ));
            assert_eq!(session.messages[3].role, MessageRole::Assistant);
            assert!(matches!(
                &session.messages[3].blocks[0],
                ContentBlock::Text { text } if text == "[Cancelled by user]"
            ));
            assert_eq!(session.messages[4].role, MessageRole::User);
            assert!(matches!(
                &session.messages[4].blocks[0],
                ContentBlock::Text { text } if text == "continue"
            ));
            assert_eq!(session.messages[5].role, MessageRole::Assistant);
            assert!(matches!(
                &session.messages[5].blocks[0],
                ContentBlock::Text { text } if text.contains("pastime")
            ));
        }
    }

    #[tokio::test]
    async fn test_cancellation_sequence_what_is_game_cancel_forget_that() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn_count = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn_count += 1;
                if turn_count == 1 {
                    // Turn 1: "what is game" -> streams partial text then cancelled
                    assert!(req.contains("what is game"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A game is an activity\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.flush().await;

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 2 {
                    // Turn 2: "forget that, what is Rust?" -> MUST see both user prompts, cancellation marker, and NO partial assistant text
                    assert!(req.contains("what is game"));
                    assert!(req.contains("[Cancelled by user]"));
                    assert!(req.contains("forget that, what is Rust?"));
                    assert!(!req.contains("A game is an activity"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Rust is a fast systems programming language.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        // Turn 1 cancelled
        let (tx1, mut rx1) = mpsc::channel(16);
        let (cancel_tx1, cancel_rx1) = watch::channel(false);
        let adapter_clone = adapter.clone();
        let turn1_task = tokio::spawn(async move {
            adapter_clone
                .execute("what is game".to_string(), tx1, cancel_rx1, 301)
                .await;
        });

        while let Some((_, ev)) = rx1.recv().await {
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("A game is") {
                    let _ = cancel_tx1.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        let _ = turn1_task.await;

        // Turn 2 "forget that, what is Rust?"
        let (tx2, mut rx2) = mpsc::channel(16);
        let (_cancel_tx2, cancel_rx2) = watch::channel(false);
        adapter
            .execute(
                "forget that, what is Rust?".to_string(),
                tx2,
                cancel_rx2,
                302,
            )
            .await;

        let mut turn2_completed = false;
        while let Some((_, ev)) = rx2.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                turn2_completed = true;
                break;
            }
        }
        assert!(turn2_completed, "Turn 2 should complete successfully");

        // Session must contain:
        // [0] User("what is game")
        // [1] Assistant("[Cancelled by user]")
        // [2] User("forget that, what is Rust?")
        // [3] Assistant("Rust is a fast systems programming language.")
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 4);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert!(matches!(
            &session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "what is game"
        ));
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[1].blocks[0],
            ContentBlock::Text { text } if text == "[Cancelled by user]"
        ));
        assert_eq!(session.messages[2].role, MessageRole::User);
        assert!(matches!(
            &session.messages[2].blocks[0],
            ContentBlock::Text { text } if text == "forget that, what is Rust?"
        ));
        assert_eq!(session.messages[3].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[3].blocks[0],
            ContentBlock::Text { text } if text.contains("systems programming")
        ));
    }

    #[tokio::test]
    async fn test_exact_real_world_rust_and_ocean_cancellation_race_sequence() {
        use crate::agent::controller::AgentController;
        use crate::app::{AgentState, App};
        use crate::command::AppCommand;
        use crate::message::Role;

        let mut app = App::new();
        let mut controller = AgentController::new();
        let (tx, mut rx) = mpsc::channel::<(usize, AgentEvent)>(32);

        // 1. User submits "what is rust in single line"
        let rust_prompt = "what is rust in single line".to_string();
        app.add_message(Role::User, rust_prompt.clone());
        let run_rust = controller.submit_prompt(rust_prompt, tx.clone());
        app.set_active_run_id(Some(run_rust));
        assert_eq!(controller.active_run_id(), Some(1));
        assert_eq!(app.active_run_id, Some(1));

        // Drain any initial events produced by MockAgent or simulate streaming
        while let Ok((id, ev)) = rx.try_recv() {
            if controller.active_run_id() == Some(id) {
                app.handle_run_event(id, ev);
            }
        }
        app.handle_run_event(
            run_rust,
            AgentEvent::TextChunk("Rust is a memory-safe language...".to_string()),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].content, "Rust is a memory-safe language...");

        // 2. User presses Ctrl+C to cancel it
        app.handle_command(AppCommand::CancelAgent);
        controller.cancel();
        assert_eq!(controller.active_run_id(), None);
        assert_eq!(controller.cancelled_run_id(), Some(run_rust));
        assert_eq!(app.active_run_id, None);
        assert_eq!(app.cancelled_run_id, Some(run_rust));
        assert_eq!(app.agent_state, AgentState::Idle);
        assert!(app.messages[1].content.ends_with("[Cancelled]"));

        // 3. User immediately submits "can you build an html page to explain importants of ocean"
        let ocean_prompt = "can you build an html page to explain importants of ocean".to_string();
        app.add_message(Role::User, ocean_prompt.clone());
        let run_ocean = controller.submit_prompt(ocean_prompt, tx.clone());
        app.set_active_run_id(Some(run_ocean));
        assert_eq!(controller.active_run_id(), Some(2));
        assert_eq!(controller.cancelled_run_id(), None);
        assert_eq!(app.active_run_id, Some(2));
        assert_eq!(app.cancelled_run_id, None);

        // 4. Stale Run A (Rust) generation produces delayed chunk & cancelled event from background task
        let stale_rust_chunk = (
            run_rust,
            AgentEvent::TextChunk(
                "Here's a clear, friendly response to your \"what is rust\" question first..."
                    .to_string(),
            ),
        );
        let stale_cancelled = (run_rust, AgentEvent::Cancelled);

        // Dispatch via main.rs event routing pattern
        for (id, ev) in [stale_rust_chunk, stale_cancelled] {
            if controller.active_run_id() == Some(id) {
                app.handle_run_event(id, ev);
            } else if controller.cancelled_run_id() == Some(id)
                && matches!(ev, AgentEvent::Cancelled)
            {
                controller.consume_cancelled();
                app.handle_run_event(id, ev);
            }
        }

        // Even if somehow passed to app.handle_run_event directly:
        app.handle_run_event(
            run_rust,
            AgentEvent::TextChunk(
                "Here's a clear, friendly response to your \"what is rust\" question first..."
                    .to_string(),
            ),
        );

        // 5. Ocean run produces its real response
        app.handle_run_event(
            run_ocean,
            AgentEvent::TextChunk(
                "<!DOCTYPE html><html><head><title>Importance of Ocean</title></head><body>Ocean produces over 50% of world oxygen.</body></html>".to_string(),
            ),
        );
        app.handle_run_event(run_ocean, AgentEvent::Completed);
        controller.mark_completed();

        // 6. Verify isolation
        assert_eq!(app.messages.len(), 4);
        assert_eq!(app.messages[0].role, Role::User);
        assert_eq!(app.messages[0].content, "what is rust in single line");
        assert_eq!(app.messages[1].role, Role::Assistant);
        assert_eq!(
            app.messages[1].content,
            "Rust is a memory-safe language...\n\n[Cancelled]"
        );
        assert_eq!(app.messages[2].role, Role::User);
        assert_eq!(
            app.messages[2].content,
            "can you build an html page to explain importants of ocean"
        );
        assert_eq!(app.messages[3].role, Role::Assistant);
        assert_eq!(
            app.messages[3].content,
            "<!DOCTYPE html><html><head><title>Importance of Ocean</title></head><body>Ocean produces over 50% of world oxygen.</body></html>"
        );
        // Stale rust response MUST NOT appear in the ocean request/response!
        assert!(!app.messages[3].content.contains("what is rust"));
        assert!(!app.messages[3].content.contains("friendly response"));
    }

    // =========================================================================
    // Regression Tests A - E: Conversation Context After Cancelled Turn
    // =========================================================================

    #[tokio::test]
    async fn test_regression_test_a_cancelled_request_followed_by_unrelated_request() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn_count = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn_count += 1;
                if turn_count == 1 {
                    // Turn 1: "what is Rust" -> completed assistant response
                    assert!(req.contains("what is Rust"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Rust is a fast systems language.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 2 {
                    // Turn 2: "build me an HTML page to explain water wastage" -> stream then cancel
                    assert!(req.contains("build me an HTML page to explain water wastage"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"<!DOCTYPE html><html>...\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.flush().await;

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 3 {
                    // Turn 3: "read this file TUI\termina\src\agent"
                    // Must verify outgoing provider request messages:
                    // User: what is Rust
                    // Assistant: Rust is a fast systems language.
                    // User: build me an HTML page to explain water wastage
                    // Assistant: [Cancelled by user]
                    // User: read this file TUI\termina\src\agent
                    assert!(req.contains("what is Rust"));
                    assert!(req.contains("Rust is a fast systems language."));
                    assert!(req.contains("build me an HTML page to explain water wastage"));
                    assert!(req.contains("[Cancelled by user]"));
                    assert!(req.contains("read this file TUI"));
                    assert!(!req.contains("<!DOCTYPE html><html>..."));

                    // Verify messages are separate alternating turns, NOT merged into consecutive user requests
                    if let Some(body_start) = req.find("\r\n\r\n") {
                        let json_body = &req[body_start + 4..];
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_body) {
                            if let Some(messages) = val.get("messages").and_then(|m| m.as_array()) {
                                assert_eq!(messages.len(), 5);
                                assert_eq!(messages[0]["role"], "user");
                                assert_eq!(messages[1]["role"], "assistant");
                                assert_eq!(messages[2]["role"], "user");
                                assert_eq!(messages[3]["role"], "assistant");
                                assert!(messages[3]["content"][0]["text"]
                                    .as_str()
                                    .unwrap()
                                    .contains("[Cancelled by user]"));
                                assert_eq!(messages[4]["role"], "user");
                                assert!(messages[4]["content"][0]["text"]
                                    .as_str()
                                    .unwrap()
                                    .contains("read this file"));
                            }
                        }
                    }

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_3\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":15,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"File contents...\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        // 1. Turn 1
        let (tx1, mut rx1) = mpsc::channel(16);
        let (_cancel_tx1, cancel_rx1) = watch::channel(false);
        adapter
            .execute("what is Rust".to_string(), tx1, cancel_rx1, 1)
            .await;
        while let Some((_, ev)) = rx1.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                break;
            }
        }

        // 2. Turn 2 (cancelled)
        let (tx2, mut rx2) = mpsc::channel(16);
        let (cancel_tx2, cancel_rx2) = watch::channel(false);
        let adapter_clone = adapter.clone();
        let turn2_task = tokio::spawn(async move {
            adapter_clone
                .execute(
                    "build me an HTML page to explain water wastage".to_string(),
                    tx2,
                    cancel_rx2,
                    2,
                )
                .await;
        });

        while let Some((_, ev)) = rx2.recv().await {
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("<!DOCTYPE") {
                    let _ = cancel_tx2.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        let _ = turn2_task.await;

        // Verify session after Turn 2 cancellation
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 4);
            assert_eq!(session.messages[0].role, MessageRole::User);
            assert_eq!(session.messages[1].role, MessageRole::Assistant);
            assert_eq!(session.messages[2].role, MessageRole::User);
            assert!(matches!(
                &session.messages[2].blocks[0],
                ContentBlock::Text { text } if text == "build me an HTML page to explain water wastage"
            ));
            assert_eq!(session.messages[3].role, MessageRole::Assistant);
            assert!(matches!(
                &session.messages[3].blocks[0],
                ContentBlock::Text { text } if text == "[Cancelled by user]"
            ));
        }

        // 3. Turn 3 unrelated request
        let (tx3, mut rx3) = mpsc::channel(16);
        let (_cancel_tx3, cancel_rx3) = watch::channel(false);
        adapter
            .execute(
                "read this file TUI\\termina\\src\\agent".to_string(),
                tx3,
                cancel_rx3,
                3,
            )
            .await;

        let mut turn3_completed = false;
        while let Some((_, ev)) = rx3.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                turn3_completed = true;
                break;
            }
        }
        assert!(turn3_completed, "Turn 3 should complete successfully");

        // Verify final session has 6 messages
        {
            let guard = adapter.session();
            let session = guard.lock().unwrap();
            assert_eq!(session.messages.len(), 6);
            assert_eq!(session.messages[4].role, MessageRole::User);
            assert!(matches!(
                &session.messages[4].blocks[0],
                ContentBlock::Text { text } if text == "read this file TUI\\termina\\src\\agent"
            ));
            assert_eq!(session.messages[5].role, MessageRole::Assistant);
            assert!(matches!(
                &session.messages[5].blocks[0],
                ContentBlock::Text { text } if text.contains("File contents")
            ));
        }
    }

    #[tokio::test]
    async fn test_regression_test_b_cancelled_request_followed_by_continue() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut turn_count = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                turn_count += 1;
                if turn_count == 1 {
                    // Turn 1: "what is game" -> stream then cancel
                    assert!(req.contains("what is game"));

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A game is an activity...\"}}\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.flush().await;

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = socket.shutdown().await;
                } else if turn_count == 2 {
                    // Turn 2: "continue" -> verify provider receives:
                    // User: what is game
                    // Assistant: [Cancelled by user]
                    // User: continue
                    assert!(req.contains("what is game"));
                    assert!(req.contains("[Cancelled by user]"));
                    assert!(req.contains("continue"));
                    assert!(!req.contains("A game is an activity..."));

                    if let Some(body_start) = req.find("\r\n\r\n") {
                        let json_body = &req[body_start + 4..];
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_body) {
                            if let Some(messages) = val.get("messages").and_then(|m| m.as_array()) {
                                assert_eq!(messages.len(), 3);
                                assert_eq!(messages[0]["role"], "user");
                                assert!(messages[0]["content"][0]["text"]
                                    .as_str()
                                    .unwrap()
                                    .contains("what is game"));
                                assert_eq!(messages[1]["role"], "assistant");
                                assert!(messages[1]["content"][0]["text"]
                                    .as_str()
                                    .unwrap()
                                    .contains("[Cancelled by user]"));
                                assert_eq!(messages[2]["role"], "user");
                                assert!(messages[2]["content"][0]["text"]
                                    .as_str()
                                    .unwrap()
                                    .contains("continue"));
                            }
                        }
                    }

                    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                    let part = concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Continuing: a game has rules and goals.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                        "data: [DONE]\n\n"
                    );
                    let _ = socket.write_all(headers.as_bytes()).await;
                    let _ = socket.write_all(part.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    break;
                }
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        // 1. Turn 1 (cancelled)
        let (tx1, mut rx1) = mpsc::channel(16);
        let (cancel_tx1, cancel_rx1) = watch::channel(false);
        let adapter_clone = adapter.clone();
        let turn1_task = tokio::spawn(async move {
            adapter_clone
                .execute("what is game".to_string(), tx1, cancel_rx1, 101)
                .await;
        });

        while let Some((_, ev)) = rx1.recv().await {
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("A game is") {
                    let _ = cancel_tx1.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        let _ = turn1_task.await;

        // 2. Turn 2 ("continue")
        let (tx2, mut rx2) = mpsc::channel(16);
        let (_cancel_tx2, cancel_rx2) = watch::channel(false);
        adapter
            .execute("continue".to_string(), tx2, cancel_rx2, 102)
            .await;

        let mut turn2_completed = false;
        while let Some((_, ev)) = rx2.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                turn2_completed = true;
                break;
            }
        }
        assert!(turn2_completed, "Turn 2 should complete successfully");

        // Verify session preserved the cancelled user prompt and cancellation marker
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 4);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert!(matches!(
            &session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "what is game"
        ));
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[1].blocks[0],
            ContentBlock::Text { text } if text == "[Cancelled by user]"
        ));
        assert_eq!(session.messages[2].role, MessageRole::User);
        assert!(matches!(
            &session.messages[2].blocks[0],
            ContentBlock::Text { text } if text == "continue"
        ));
        assert_eq!(session.messages[3].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[3].blocks[0],
            ContentBlock::Text { text } if text.contains("Continuing")
        ));
    }

    #[tokio::test]
    async fn test_regression_test_c_incomplete_assistant_output_never_persisted() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                let part = concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_c\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                    "event: content_block_start\n",
                    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"PartialChunkAlpha \"}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"PartialChunkBeta\"}}\n\n"
                );
                let _ = socket.write_all(headers.as_bytes()).await;
                let _ = socket.write_all(part.as_bytes()).await;
                let _ = socket.flush().await;

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = Arc::new(RealAgentAdapter::with_client(provider));

        let (tx, mut rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let adapter_clone = adapter.clone();
        let prompt_task = tokio::spawn(async move {
            adapter_clone
                .execute(
                    "test prompt for incomplete stream".to_string(),
                    tx,
                    cancel_rx,
                    201,
                )
                .await;
        });

        let mut received_partial = false;
        while let Some((_, ev)) = rx.recv().await {
            if let AgentEvent::TextChunk(text) = ev {
                if text.contains("PartialChunkAlpha") {
                    received_partial = true;
                    let _ = cancel_tx.send(true);
                }
            } else if let AgentEvent::Cancelled = ev {
                break;
            }
        }
        assert!(
            received_partial,
            "Should have received partial streamed text"
        );
        let _ = prompt_task.await;

        // Session must contain user prompt and [Cancelled by user], but NOT partial generation
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert!(matches!(
            &session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "test prompt for incomplete stream"
        ));
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[1].blocks[0],
            ContentBlock::Text { text } if text == "[Cancelled by user]"
        ));

        for msg in &session.messages {
            for block in &msg.blocks {
                if let ContentBlock::Text { text } = block {
                    assert!(!text.contains("PartialChunkAlpha"));
                    assert!(!text.contains("PartialChunkBeta"));
                }
            }
        }
    }

    #[tokio::test]
    async fn test_regression_test_d_failed_turn_does_not_add_cancellation_marker() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                // Return 500 Internal Server Error
                let body = r#"{"error":{"message":"Internal server error"}}"#;
                let resp = format!(
                    "HTTP/1.1 500 Internal Server Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("failing prompt".to_string(), tx, cancel_rx, 401)
            .await;

        let mut got_error = false;
        while let Some((_, ev)) = rx.recv().await {
            if matches!(ev, AgentEvent::Error(_)) {
                got_error = true;
                break;
            }
        }
        assert!(got_error, "Should have received an Error event");

        // Session must NOT contain any synthetic cancellation marker
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 0);
        assert!(!session
            .messages
            .iter()
            .any(|m| m.blocks.iter().any(|b| match b {
                ContentBlock::Text { text } => text.contains("[Cancelled by user]"),
                _ => false,
            })));
    }

    #[tokio::test]
    async fn test_regression_test_e_successful_turn_remains_unchanged() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                let part = concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_succ\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                    "event: content_block_start\n",
                    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Actual assistant response.\"}}\n\n",
                    "event: content_block_stop\n",
                    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n",
                    "data: [DONE]\n\n"
                );
                let _ = socket.write_all(headers.as_bytes()).await;
                let _ = socket.write_all(part.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Hello from user".to_string(), tx, cancel_rx, 501)
            .await;

        let mut completed = false;
        while let Some((_, ev)) = rx.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                completed = true;
                break;
            }
        }
        assert!(completed, "Successful turn must complete");

        // Normal completed prompt must produce User -> Assistant actual response with no cancellation marker
        let guard = adapter.session();
        let session = guard.lock().unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert!(matches!(
            &session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "Hello from user"
        ));
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &session.messages[1].blocks[0],
            ContentBlock::Text { text } if text == "Actual assistant response."
        ));

        assert!(!session
            .messages
            .iter()
            .any(|m| m.blocks.iter().any(|b| match b {
                ContentBlock::Text { text } => text.contains("[Cancelled by user]"),
                _ => false,
            })));
    }

    #[test]
    fn test_real_agent_adapter_with_persistent_session_preserves_metadata() {
        let temp_dir =
            std::env::temp_dir().join(format!("crudo_real_persist_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");

        let coordinator =
            crate::session::SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");
        let mut session = coordinator.active_session().clone();
        session
            .push_user_text("Prior question")
            .expect("push user text");
        let _ = session.push_prompt_entry("Prior prompt");

        let adapter = RealAgentAdapter::new().with_session(session.clone());
        let adapter_session = adapter.get_session();

        assert_eq!(adapter_session.session_id, session.session_id);
        assert_eq!(
            adapter_session.persistence_path(),
            session.persistence_path()
        );
        assert_eq!(adapter_session.workspace_root(), session.workspace_root());
        assert_eq!(adapter_session.messages, session.messages);
        assert_eq!(adapter_session.prompt_history, session.prompt_history);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_real_agent_adapter_set_session_updates_in_place() {
        let temp_dir =
            std::env::temp_dir().join(format!("crudo_real_set_session_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");

        let adapter = RealAgentAdapter::new();
        let initial_id = adapter.get_session().session_id;

        let coordinator =
            crate::session::SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");
        let persistent_session = coordinator.active_session().clone();

        assert_ne!(initial_id, persistent_session.session_id);
        adapter.set_session(persistent_session.clone());

        let current = adapter.get_session();
        assert_eq!(current.session_id, persistent_session.session_id);
        assert_eq!(
            current.persistence_path(),
            persistent_session.persistence_path()
        );
        assert_eq!(
            current.workspace_root(),
            persistent_session.workspace_root()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_real_agent_adapter_execution_preserves_persistent_session_and_writes_to_disk() {
        let temp_dir =
            std::env::temp_dir().join(format!("crudo_real_exec_persist_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");

        let coordinator =
            crate::session::SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");
        let initial_session = coordinator.active_session().clone();
        let expected_session_id = initial_session.session_id.clone();
        let expected_persistence_path = initial_session
            .persistence_path()
            .expect("persistence path")
            .to_path_buf();
        let expected_workspace_root = initial_session
            .workspace_root()
            .expect("workspace root")
            .to_path_buf();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let Ok(n) = socket.read(&mut chunk).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req = String::from_utf8_lossy(&buffer);
                if req.contains("/count_tokens") {
                    let body = r#"{"input_tokens":5}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                let part = concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_persist\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                    "event: content_block_start\n",
                    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Persistent response.\"}}\n\n",
                    "event: content_block_stop\n",
                    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n",
                    "data: [DONE]\n\n"
                );
                let _ = socket.write_all(headers.as_bytes()).await;
                let _ = socket.write_all(part.as_bytes()).await;
                let _ = socket.shutdown().await;
                break;
            }
        });

        let anthropic = AnthropicClient::new("test-key")
            .with_base_url(format!("http://{addr}"))
            .with_retry_policy(0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let provider = ProviderClient::Anthropic(anthropic);
        let adapter = RealAgentAdapter::with_client(provider).with_session(initial_session);

        let (tx, mut rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        adapter
            .execute("Prompt for persistent turn".to_string(), tx, cancel_rx, 701)
            .await;

        let mut completed = false;
        while let Some((_, ev)) = rx.recv().await {
            if matches!(ev, AgentEvent::Completed) {
                completed = true;
                break;
            }
        }
        assert!(completed, "Turn should complete successfully");

        // Verify in-memory adapter session retained all persistence metadata
        let current_session = adapter.get_session();
        assert_eq!(current_session.session_id, expected_session_id);
        assert_eq!(
            current_session.persistence_path(),
            Some(expected_persistence_path.as_path())
        );
        assert_eq!(
            current_session.workspace_root(),
            Some(expected_workspace_root.as_path())
        );
        assert_eq!(current_session.messages.len(), 2);

        // Verify incremental JSONL file was written to disk
        assert!(
            expected_persistence_path.exists(),
            "Session JSONL file must exist on disk"
        );
        let disk_content =
            std::fs::read_to_string(&expected_persistence_path).expect("read session jsonl");
        assert!(
            disk_content.contains("Prompt for persistent turn"),
            "JSONL file must contain user prompt"
        );
        assert!(
            disk_content.contains("Persistent response."),
            "JSONL file must contain assistant response"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_core_profile_returns_exactly_four_tools_preserving_order() {
        let registry = GlobalToolRegistry::builtin();
        let all_tools = registry.definitions(None);
        assert!(
            all_tools.len() > 4,
            "Builtin tool registry should have more than 4 tools, got {}",
            all_tools.len()
        );

        // Test with "core"
        let filtered = filter_tool_definitions_with_lookup(all_tools.clone(), |k| {
            (k == "CRUDO_TOOL_PROFILE").then(|| "core".to_string())
        });
        assert_eq!(filtered.len(), 4);
        let names: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();
        // Preserves original registry order of the core tools (bash, read_file, write_file, edit_file)
        assert_eq!(names, vec!["bash", "read_file", "write_file", "edit_file"]);

        // Test case-insensitivity and trimming
        for variant in &["CORE", "Core", " core ", "  CORE  "] {
            let res = filter_tool_definitions_with_lookup(all_tools.clone(), |k| {
                (k == "CRUDO_TOOL_PROFILE").then(|| (*variant).to_string())
            });
            assert_eq!(
                res.len(),
                4,
                "variant `{variant}` should be accepted as core profile"
            );
            let res_names: Vec<&str> = res.iter().map(|t| t.name.as_str()).collect();
            assert_eq!(
                res_names,
                vec!["bash", "read_file", "write_file", "edit_file"]
            );
        }
    }

    #[test]
    fn test_default_or_absent_profile_returns_full_tool_set() {
        let registry = GlobalToolRegistry::builtin();
        let all_tools = registry.definitions(None);
        let count = all_tools.len();

        // Absent (None)
        let res_none = filter_tool_definitions_with_lookup(all_tools.clone(), |_| None);
        assert_eq!(res_none.len(), count);
        assert_eq!(
            res_none.iter().map(|t| &t.name).collect::<Vec<_>>(),
            all_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );

        // Empty string
        let res_empty = filter_tool_definitions_with_lookup(all_tools.clone(), |k| {
            (k == "CRUDO_TOOL_PROFILE").then(|| "".to_string())
        });
        assert_eq!(res_empty.len(), count);

        // Whitespace only
        let res_spaces = filter_tool_definitions_with_lookup(all_tools.clone(), |k| {
            (k == "CRUDO_TOOL_PROFILE").then(|| "   ".to_string())
        });
        assert_eq!(res_spaces.len(), count);
    }

    #[test]
    fn test_invalid_profile_falls_back_to_full_tool_set() {
        let registry = GlobalToolRegistry::builtin();
        let all_tools = registry.definitions(None);
        let count = all_tools.len();

        for invalid in &[
            "all", "extended", "full", "custom", "1", "false", "cor", "coree",
        ] {
            let res = filter_tool_definitions_with_lookup(all_tools.clone(), |k| {
                (k == "CRUDO_TOOL_PROFILE").then(|| (*invalid).to_string())
            });
            assert_eq!(
                res.len(),
                count,
                "invalid profile `{invalid}` must fall back to full tool set"
            );
        }
    }

    #[test]
    fn test_tool_execution_still_uses_full_global_tool_registry() {
        let registry = GlobalToolRegistry::builtin();

        // Verify that non-core tools (like glob_search) remain executable in the registry
        // even when a core profile is selected for model definitions
        let (tx, _rx) = mpsc::channel(16);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let mut executor = TuiToolExecutor {
            registry: registry.clone(),
            tx,
            run_id: 1,
            next_tool_id: 0,
            cancel_rx,
        };

        // Sleep is NOT in CORE_PROFILE_TOOLS, but GlobalToolRegistry must still execute it
        let res = executor.execute("Sleep", r#"{"duration_ms":1}"#);
        assert!(
            res.is_ok(),
            "Non-core tools must remain executable in GlobalToolRegistry: {:?}",
            res.err()
        );
    }

    #[test]
    fn test_core_profile_permissions_not_bypassed() {
        use runtime::{PermissionMode, PermissionOutcome};

        // In ReadOnly mode, write_file and edit_file must still be blocked by policy/enforcer
        let mut policy = PermissionPolicy::new(PermissionMode::ReadOnly);
        let registry = GlobalToolRegistry::builtin();
        for (name, req_mode) in registry.permission_specs(None).unwrap() {
            policy = policy.with_tool_requirement(name, req_mode);
        }

        // read_file is allowed under ReadOnly
        let read_outcome = policy.authorize("read_file", r#"{"path":"Cargo.toml"}"#, None);
        assert!(
            matches!(read_outcome, PermissionOutcome::Allow),
            "read_file should be allowed in ReadOnly mode"
        );

        // write_file requires WorkspaceWrite, so must not be allowed in ReadOnly mode
        let write_outcome = policy.authorize(
            "write_file",
            r#"{"path":"test.txt","content":"hello"}"#,
            None,
        );
        assert!(
            !matches!(write_outcome, PermissionOutcome::Allow),
            "write_file must not bypass permission check in ReadOnly mode"
        );

        // edit_file requires WorkspaceWrite, so must not be allowed in ReadOnly mode
        let edit_outcome = policy.authorize(
            "edit_file",
            r#"{"path":"test.txt","old_string":"a","new_string":"b"}"#,
            None,
        );
        assert!(
            !matches!(edit_outcome, PermissionOutcome::Allow),
            "edit_file must not bypass permission check in ReadOnly mode"
        );
    }
}
