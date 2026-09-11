use std::fmt;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub provider: String,
    pub endpoint: String,
    pub active_model: Option<String>,
    pub timeout: Duration,
    pub streaming_enabled: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            active_model: None,
            timeout: Duration::from_secs(30),
            streaming_enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModelError {
    NotConfigured,
    ModelNotFound(String),
    ConnectionFailed(String),
    Timeout,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "Model provider is not configured"),
            Self::ModelNotFound(m) => write!(f, "Model not found: {m}"),
            Self::ConnectionFailed(e) => write!(f, "Model connection failed: {e}"),
            Self::Timeout => write!(f, "Model request timed out"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Abstraction for pluggable local model engines (Ollama, llama.cpp, vLLM, OpenAI-compatible local APIs).
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    /// Lists all models locally installed / available in this provider.
    async fn list_models(&self) -> Result<Vec<String>, ModelError>;

    /// Generates a non-streaming response.
    async fn generate(&self, model: &str, prompt: &str) -> Result<String, ModelError>;

    /// Checks the health of the local inference endpoint.
    async fn health(&self) -> Result<bool, ModelError>;

    /// Returns the provider configuration.
    fn config(&self) -> &ModelConfig;
}

/// Truthful disconnected model provider when no local server has been configured.
pub struct DisconnectedModelProvider {
    config: ModelConfig,
}

impl Default for DisconnectedModelProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DisconnectedModelProvider {
    pub fn new() -> Self {
        Self {
            config: ModelConfig::default(),
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for DisconnectedModelProvider {
    async fn list_models(&self) -> Result<Vec<String>, ModelError> {
        Err(ModelError::NotConfigured)
    }

    async fn generate(&self, _model: &str, _prompt: &str) -> Result<String, ModelError> {
        Err(ModelError::NotConfigured)
    }

    async fn health(&self) -> Result<bool, ModelError> {
        Ok(false)
    }

    fn config(&self) -> &ModelConfig {
        &self.config
    }
}

pub type SharedModelProvider = Arc<dyn ModelProvider>;
