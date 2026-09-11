use chrono::{DateTime, Local};
use std::fmt;

/// The only two actors visible in the user-facing conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User,
    Crudo,
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "USER"),
            Self::Crudo => write!(f, "CRUDO"),
        }
    }
}

/// Stages of the document ingestion pipeline (e.g. Docling / OCR / Local Parsers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStage {
    Reading,
    Parsing,
    OCR,
    Extracting,
    Indexing,
    Completed,
    Failed,
}

impl fmt::Display for DocumentStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reading => write!(f, "Reading"),
            Self::Parsing => write!(f, "Parsing"),
            Self::OCR => write!(f, "OCR"),
            Self::Extracting => write!(f, "Extracting"),
            Self::Indexing => write!(f, "Indexing"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Truthful status of the CRUDO backend connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendConnectionStatus {
    NotConnected,
    Connecting,
    Connected,
    Error(String),
}

impl fmt::Display for BackendConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "NOT CONNECTED"),
            Self::Connecting => write!(f, "CONNECTING"),
            Self::Connected => write!(f, "CONNECTED"),
            Self::Error(_) => write!(f, "ERROR"),
        }
    }
}

/// Truthful status of the local Model subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    NotConnected,
    Loaded(String),
    Generating(String),
    Error(String),
}

impl fmt::Display for ModelStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "NOT CONNECTED"),
            Self::Loaded(m) => write!(f, "{m}"),
            Self::Generating(m) => write!(f, "{m} (BUSY)"),
            Self::Error(_) => write!(f, "ERROR"),
        }
    }
}

/// State-driven MCP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    NotConnected,
    Off,
    Connecting,
    Ready,
    Busy,
    Error(String),
}

impl fmt::Display for McpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "NOT CONNECTED"),
            Self::Off => write!(f, "OFF"),
            Self::Connecting => write!(f, "CONNECTING"),
            Self::Ready => write!(f, "READY"),
            Self::Busy => write!(f, "BUSY"),
            Self::Error(_) => write!(f, "ERROR"),
        }
    }
}

/// State-driven Sandbox status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxStatus {
    NotConnected,
    NotReady,
    Starting,
    Ready,
    Busy,
    Error(String),
}

impl fmt::Display for SandboxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "NOT CONNECTED"),
            Self::NotReady => write!(f, "NOT READY"),
            Self::Starting => write!(f, "STARTING"),
            Self::Ready => write!(f, "READY"),
            Self::Busy => write!(f, "BUSY"),
            Self::Error(_) => write!(f, "ERROR"),
        }
    }
}

/// Events emitted by the CRUDO backend or internal subsystems.
#[derive(Debug, Clone)]
pub enum CrudoEvent {
    // Conversational events
    MessageCreated {
        role: Actor,
        content: String,
        timestamp: DateTime<Local>,
    },
    ResponseStarted {
        message_id: String,
    },
    ResponseDelta {
        message_id: String,
        delta: String,
    },
    ResponseCompleted {
        message_id: String,
    },
    ResponseFailed {
        message_id: String,
        error: String,
    },

    // Document processing events
    DocumentStarted {
        document_id: String,
        filename: String,
    },
    DocumentProgress {
        document_id: String,
        filename: String,
        stage: DocumentStage,
        current: u64,
        total: u64,
        percentage: u8,
    },
    DocumentCompleted {
        document_id: String,
        filename: String,
    },
    DocumentFailed {
        document_id: String,
        filename: String,
        error: String,
    },

    // Vision / Image processing events
    VisionStarted {
        image_id: String,
        filename: String,
    },
    VisionProgress {
        image_id: String,
        filename: String,
        stage: String,
        percentage: u8,
    },
    VisionCompleted {
        image_id: String,
        filename: String,
    },
    VisionFailed {
        image_id: String,
        filename: String,
        error: String,
    },

    // Subsystem activity events (MCP, Tools, Sandbox, Models)
    AgentActivity(AgentEvent),

    // Subsystem status updates
    BackendStatusChanged(BackendConnectionStatus),
    ModelStatusChanged(ModelStatus),
    McpStatusChanged(McpStatus),
    SandboxStatusChanged(SandboxStatus),
}

/// Detailed execution event for agent operations.
/// These belong to the execution/activity layer and are NEVER displayed as normal chat messages.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    ModelStarted {
        model: String,
    },
    ModelProgress {
        model: String,
        tokens: usize,
    },
    ModelCompleted {
        model: String,
    },
    ModelFailed {
        model: String,
        error: String,
    },

    McpStarted {
        server: String,
        method: String,
    },
    McpCompleted {
        server: String,
        method: String,
        summary: Option<String>,
    },
    McpFailed {
        server: String,
        method: String,
        error: String,
    },

    ToolStarted {
        tool_name: String,
    },
    ToolProgress {
        tool_name: String,
        stage: String,
    },
    ToolCompleted {
        tool_name: String,
    },
    ToolFailed {
        tool_name: String,
        error: String,
    },

    SandboxStarted {
        task_id: String,
    },
    SandboxCompleted {
        task_id: String,
    },
    SandboxFailed {
        task_id: String,
        error: String,
    },

    ArtifactCreated {
        title: String,
        path: Option<String>,
    },
    FileCreated {
        path: String,
        size_bytes: u64,
    },
    SystemNotification(String),
}
