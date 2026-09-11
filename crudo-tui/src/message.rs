#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Complete,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: usize,
    pub role: Role,
    pub content: String,
    pub status: MessageStatus,
}

impl Message {
    pub fn new(id: usize, role: Role, content: String) -> Self {
        Self {
            id,
            role,
            content,
            status: MessageStatus::Pending,
        }
    }

    pub fn completed(id: usize, role: Role, content: String) -> Self {
        Self {
            id,
            role,
            content,
            status: MessageStatus::Complete,
        }
    }
}
