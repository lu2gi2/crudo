use std::collections::BTreeMap;

use runtime::{pricing_for_model, TokenUsage, UsageCostEstimate};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MessageRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<InputMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
    /// OpenAI-compatible tuning parameters. Optional — omitted from payload when None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Reasoning effort level for OpenAI-compatible reasoning models (e.g. `o4-mini`).
    /// Accepted values: `"low"`, `"medium"`, `"high"`. Omitted when `None`.
    /// Silently ignored by backends that do not support it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Provider-specific OpenAI-compatible request body parameters. These are
    /// copied into the final JSON payload after core fields are populated so
    /// users can opt into gateway features such as `web_search_options`,
    /// `parallel_tool_calls`, or custom local-server switches without waiting
    /// for first-class typed fields. Core protocol keys are protected and cannot
    /// be overridden through this map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_body: BTreeMap<String, Value>,
}

impl MessageRequest {
    #[must_use]
    pub fn with_streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    /// Return a copy of this request with tool definitions stripped if `CRUDO_DISABLE_TOOLS` is active.
    #[must_use]
    pub fn maybe_strip_tools(&self) -> Self {
        self.maybe_strip_tools_with_lookup(|key| std::env::var(key).ok())
    }

    /// Return a copy of this request with tool definitions stripped according to the provided env lookup.
    #[must_use]
    pub fn maybe_strip_tools_with_lookup<F>(&self, lookup: F) -> Self
    where
        F: FnMut(&str) -> Option<String>,
    {
        if should_disable_tools_from_lookup(lookup) {
            Self {
                tools: None,
                tool_choice: None,
                ..self.clone()
            }
        } else {
            self.clone()
        }
    }
}

/// Check if tool definitions should be disabled for outgoing provider requests.
///
/// Returns true when `CRUDO_DISABLE_TOOLS` is set to `"1"`, `"true"`, or `"TRUE"`.
/// When absent or any other value, returns false.
#[must_use]
pub fn should_disable_tools() -> bool {
    should_disable_tools_from_lookup(|key| std::env::var(key).ok())
}

/// Check if tool definitions should be disabled given an environment lookup function.
#[must_use]
pub fn should_disable_tools_from_lookup<F>(mut lookup: F) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    lookup("CRUDO_DISABLE_TOOLS")
        .as_deref()
        .map(str::trim)
        .is_some_and(|val| val == "1" || val.eq_ignore_ascii_case("true"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputMessage {
    pub role: String,
    pub content: Vec<InputContentBlock>,
}

impl InputMessage {
    #[must_use]
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text { text: text.into() }],
        }
    }

    #[must_use]
    pub fn user_tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![InputContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: vec![ToolResultContentBlock::Text {
                    text: content.into(),
                }],
                is_error,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<ToolResultContentBlock>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContentBlock {
    Text { text: String },
    Json { value: Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub role: String,
    pub content: Vec<OutputContentBlock>,
    pub model: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub request_id: Option<String>,
}

impl MessageResponse {
    #[must_use]
    pub fn total_tokens(&self) -> u32 {
        self.usage.total_tokens()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    RedactedThinking {
        data: Value,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
}

impl Usage {
    #[must_use]
    pub const fn total_tokens(&self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    #[must_use]
    pub const fn token_usage(&self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation_input_tokens: self.cache_creation_input_tokens,
            cache_read_input_tokens: self.cache_read_input_tokens,
        }
    }

    #[must_use]
    pub fn estimated_cost_usd(&self, model: &str) -> UsageCostEstimate {
        let usage = self.token_usage();
        pricing_for_model(model).map_or_else(
            || usage.estimate_cost_usd(),
            |pricing| usage.estimate_cost_usd_with_pricing(pricing),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageStartEvent {
    pub message: MessageResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDeltaEvent {
    pub delta: MessageDelta,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDelta {
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlockStartEvent {
    pub index: u32,
    pub content_block: OutputContentBlock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlockDeltaEvent {
    pub index: u32,
    pub delta: ContentBlockDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentBlockStopEvent {
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageStopEvent {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart(MessageStartEvent),
    MessageDelta(MessageDeltaEvent),
    ContentBlockStart(ContentBlockStartEvent),
    ContentBlockDelta(ContentBlockDeltaEvent),
    ContentBlockStop(ContentBlockStopEvent),
    MessageStop(MessageStopEvent),
}

#[cfg(test)]
mod tests {
    use runtime::format_usd;
    use serde_json::json;

    use super::{
        should_disable_tools_from_lookup, InputContentBlock, MessageRequest, MessageResponse,
        ToolChoice, ToolDefinition, Usage,
    };

    #[test]
    fn usage_total_tokens_includes_cache_tokens() {
        let usage = Usage {
            input_tokens: 10,
            cache_creation_input_tokens: 2,
            cache_read_input_tokens: 3,
            output_tokens: 4,
        };

        assert_eq!(usage.total_tokens(), 19);
        assert_eq!(usage.token_usage().total_tokens(), 19);
    }

    #[test]
    fn message_response_estimates_cost_from_model_usage() {
        let response = MessageResponse {
            id: "msg_cost".to_string(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: Vec::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: Usage {
                input_tokens: 1_000_000,
                cache_creation_input_tokens: 100_000,
                cache_read_input_tokens: 200_000,
                output_tokens: 500_000,
            },
            request_id: None,
        };

        let cost = response.usage.estimated_cost_usd(&response.model);
        assert_eq!(format_usd(cost.total_cost_usd()), "$54.6750");
        assert_eq!(response.total_tokens(), 1_800_000);
    }

    #[test]
    fn input_content_block_thinking_serializes_with_snake_case_type() {
        // given
        let block = InputContentBlock::Thinking {
            thinking: "pondering".to_string(),
            signature: Some("sig_123".to_string()),
        };

        // when
        let serialized = serde_json::to_value(&block).unwrap();
        let deserialized: InputContentBlock = serde_json::from_value(json!({
            "type": "thinking",
            "thinking": "pondering",
            "signature": "sig_123"
        }))
        .unwrap();

        // then
        assert_eq!(
            serialized,
            json!({
                "type": "thinking",
                "thinking": "pondering",
                "signature": "sig_123"
            })
        );
        assert_eq!(deserialized, block);
    }

    #[test]
    fn tools_remain_enabled_by_default() {
        let lookup = |_: &str| None;
        assert!(!should_disable_tools_from_lookup(lookup));

        let req = MessageRequest {
            tools: Some(vec![ToolDefinition {
                name: "bash".to_string(),
                description: Some("execute command".to_string()),
                input_schema: json!({}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        let stripped = req.maybe_strip_tools_with_lookup(lookup);
        assert!(stripped.tools.is_some());
        assert_eq!(stripped.tools.as_ref().unwrap().len(), 1);
        assert!(stripped.tool_choice.is_some());
    }

    #[test]
    fn crudo_disable_tools_1_disables_tools() {
        let lookup = |key: &str| (key == "CRUDO_DISABLE_TOOLS").then(|| "1".to_string());
        assert!(should_disable_tools_from_lookup(lookup));

        let req = MessageRequest {
            tools: Some(vec![ToolDefinition {
                name: "bash".to_string(),
                description: Some("execute command".to_string()),
                input_schema: json!({}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        let stripped = req.maybe_strip_tools_with_lookup(lookup);
        assert!(stripped.tools.is_none());
        assert!(stripped.tool_choice.is_none());
    }

    #[test]
    fn crudo_disable_tools_true_disables_tools() {
        let lookup_lower = |key: &str| (key == "CRUDO_DISABLE_TOOLS").then(|| "true".to_string());
        assert!(should_disable_tools_from_lookup(lookup_lower));

        let lookup_upper = |key: &str| (key == "CRUDO_DISABLE_TOOLS").then(|| "TRUE".to_string());
        assert!(should_disable_tools_from_lookup(lookup_upper));

        let lookup_mixed = |key: &str| (key == "CRUDO_DISABLE_TOOLS").then(|| "True".to_string());
        assert!(should_disable_tools_from_lookup(lookup_mixed));

        let req = MessageRequest {
            tools: Some(vec![ToolDefinition {
                name: "bash".to_string(),
                description: Some("execute command".to_string()),
                input_schema: json!({}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        let stripped = req.maybe_strip_tools_with_lookup(lookup_upper);
        assert!(stripped.tools.is_none());
        assert!(stripped.tool_choice.is_none());
    }

    #[test]
    fn invalid_value_does_not_disable_tools() {
        for invalid in &["0", "false", "FALSE", "no", "invalid", "", "2", "random"] {
            let lookup = |key: &str| (key == "CRUDO_DISABLE_TOOLS").then(|| (*invalid).to_string());
            assert!(
                !should_disable_tools_from_lookup(lookup),
                "value `{invalid}` should not disable tools"
            );

            let req = MessageRequest {
                tools: Some(vec![ToolDefinition {
                    name: "bash".to_string(),
                    description: Some("execute command".to_string()),
                    input_schema: json!({}),
                }]),
                tool_choice: Some(ToolChoice::Auto),
                ..Default::default()
            };
            let stripped = req.maybe_strip_tools_with_lookup(lookup);
            assert!(stripped.tools.is_some());
            assert!(stripped.tool_choice.is_some());
        }
    }

    #[test]
    fn tool_definitions_are_still_generated_normally_when_disabled_mode_is_off() {
        let lookup = |_: &str| None;
        let req = MessageRequest {
            tools: Some(vec![
                ToolDefinition {
                    name: "read_file".to_string(),
                    description: Some("Read a file".to_string()),
                    input_schema: json!({"type": "object"}),
                },
                ToolDefinition {
                    name: "write_file".to_string(),
                    description: Some("Write a file".to_string()),
                    input_schema: json!({"type": "object"}),
                },
            ]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        let processed = req.maybe_strip_tools_with_lookup(lookup);
        assert_eq!(processed.tools.as_ref().unwrap().len(), 2);
        assert_eq!(processed.tools.as_ref().unwrap()[0].name, "read_file");
        assert_eq!(processed.tools.as_ref().unwrap()[1].name, "write_file");
    }
}
