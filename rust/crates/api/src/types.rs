use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Explicit provider-neutral thinking mode: `on`, `off`, or `auto`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_mode: Option<String>,
    /// Provider-specific OpenAI-compatible request body parameters. These are
    /// copied into the final JSON payload after core fields are populated so
    /// users can opt into gateway features such as `web_search_options`,
    /// `parallel_tool_calls`, or custom local-server switches without waiting
    /// for first-class typed fields. Core protocol keys are protected and cannot
    /// be overridden through this map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_body: BTreeMap<String, Value>,
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validates_supported_local_attachment() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("diagram.png");
        fs::write(&path, b"image").unwrap();
        let attachment = MultimodalAttachment::from_path(&path, dir.path()).unwrap();
        assert_eq!(attachment.kind, AttachmentKind::Image);
        assert_eq!(attachment.media_type, "image/png");
        assert!(attachment.local_only);
        assert_eq!(attachment.size_bytes, 5);
        assert_eq!(attachment.content_hash.len(), 64);
    }

    #[test]
    fn rejects_attachment_outside_workspace() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let path = outside.path().join("secret.pdf");
        fs::write(&path, b"secret").unwrap();
        assert!(matches!(
            MultimodalAttachment::from_path(&path, dir.path()),
            Err(AttachmentValidationError::OutsideWorkspace(_))
        ));
    }

    #[test]
    fn rejects_unknown_attachment_type() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        fs::write(&path, b"bytes").unwrap();
        assert!(matches!(
            MultimodalAttachment::from_path(&path, dir.path()),
            Err(AttachmentValidationError::UnsupportedType(_))
        ));
    }
}

impl MessageRequest {
    #[must_use]
    pub fn with_streaming(mut self) -> Self {
        self.stream = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Pdf,
    Document,
    Spreadsheet,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultimodalAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub media_type: String,
    pub kind: AttachmentKind,
    pub size_bytes: u64,
    pub content_hash: String,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentValidationError {
    NotAFile(PathBuf),
    OutsideWorkspace(PathBuf),
    TooLarge {
        path: PathBuf,
        size_bytes: u64,
        max_bytes: u64,
    },
    UnsupportedType(PathBuf),
    Io(String),
}

impl MultimodalAttachment {
    pub fn from_path(
        path: impl AsRef<Path>,
        workspace: impl AsRef<Path>,
    ) -> Result<Self, AttachmentValidationError> {
        const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;
        let path = path
            .as_ref()
            .canonicalize()
            .map_err(|e| AttachmentValidationError::Io(e.to_string()))?;
        let workspace = workspace
            .as_ref()
            .canonicalize()
            .map_err(|e| AttachmentValidationError::Io(e.to_string()))?;
        if !path.starts_with(&workspace) {
            return Err(AttachmentValidationError::OutsideWorkspace(path));
        }
        let metadata =
            fs::metadata(&path).map_err(|e| AttachmentValidationError::Io(e.to_string()))?;
        if !metadata.is_file() {
            return Err(AttachmentValidationError::NotAFile(path));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(AttachmentValidationError::TooLarge {
                path,
                size_bytes: metadata.len(),
                max_bytes: MAX_ATTACHMENT_BYTES,
            });
        }
        let (kind, media_type) = attachment_type(&path)
            .ok_or_else(|| AttachmentValidationError::UnsupportedType(path.clone()))?;
        let bytes = fs::read(&path).map_err(|e| AttachmentValidationError::Io(e.to_string()))?;
        Ok(Self {
            filename: path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("attachment")
                .to_string(),
            path,
            media_type: media_type.to_string(),
            kind,
            size_bytes: metadata.len(),
            content_hash: blake3::hash(&bytes).to_hex().to_string(),
            local_only: true,
        })
    }
}

fn attachment_type(path: &Path) -> Option<(AttachmentKind, &'static str)> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some((AttachmentKind::Image, "image/png")),
        "jpg" | "jpeg" => Some((AttachmentKind::Image, "image/jpeg")),
        "webp" => Some((AttachmentKind::Image, "image/webp")),
        "pdf" => Some((AttachmentKind::Pdf, "application/pdf")),
        "doc" => Some((AttachmentKind::Document, "application/msword")),
        "docx" => Some((
            AttachmentKind::Document,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )),
        "txt" => Some((AttachmentKind::Text, "text/plain")),
        "md" => Some((AttachmentKind::Text, "text/markdown")),
        "csv" => Some((AttachmentKind::Spreadsheet, "text/csv")),
        "xls" => Some((AttachmentKind::Spreadsheet, "application/vnd.ms-excel")),
        "xlsx" => Some((
            AttachmentKind::Spreadsheet,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputMessage {
    pub role: String,
    pub content: Vec<InputContentBlock>,
}

impl MultimodalAttachment {
    #[must_use]
    pub fn base64_data(&self) -> Result<String, AttachmentValidationError> {
        let data =
            fs::read(&self.path).map_err(|e| AttachmentValidationError::Io(e.to_string()))?;
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(data))
    }
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
    pub fn user_image(
        attachment: &MultimodalAttachment,
    ) -> Result<Self, AttachmentValidationError> {
        Ok(Self {
            role: "user".to_string(),
            content: vec![InputContentBlock::Image {
                media_type: attachment.media_type.clone(),
                data: attachment.base64_data()?,
            }],
        })
    }

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
    Image {
        media_type: String,
        data: String,
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

    use super::{InputContentBlock, MessageResponse, Usage};

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
}
