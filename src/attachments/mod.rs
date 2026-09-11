use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    Pdf,
    Image,
    Document,
    Spreadsheet,
    CadDrawing,
    Text,
    Binary,
}

impl fmt::Display for AttachmentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pdf => write!(f, "PDF Document"),
            Self::Image => write!(f, "Image / Photo"),
            Self::Document => write!(f, "Word Document"),
            Self::Spreadsheet => write!(f, "Spreadsheet"),
            Self::CadDrawing => write!(f, "CAD / P&ID Drawing"),
            Self::Text => write!(f, "Text File"),
            Self::Binary => write!(f, "Binary File"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub path: PathBuf,
    pub filename: String,
    pub extension: String,
    pub size_bytes: u64,
    pub kind: AttachmentKind,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    NotFound(PathBuf),
    NotAFile(PathBuf),
    IoError(String),
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "File not found: {}", p.display()),
            Self::NotAFile(p) => write!(f, "Path is not a regular file: {}", p.display()),
            Self::IoError(e) => write!(f, "Failed to read file metadata: {e}"),
        }
    }
}

impl std::error::Error for AttachmentError {}

impl Attachment {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, AttachmentError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(AttachmentError::NotFound(p.to_path_buf()));
        }

        let metadata = fs::metadata(p).map_err(|e| AttachmentError::IoError(e.to_string()))?;
        if !metadata.is_file() {
            return Err(AttachmentError::NotAFile(p.to_path_buf()));
        }

        let filename = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let extension = p
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let (kind, mime_type) = Self::classify(&extension);
        let size_bytes = metadata.len();

        Ok(Self {
            path: p.to_path_buf(),
            filename,
            extension,
            size_bytes,
            kind,
            mime_type: mime_type.to_string(),
        })
    }

    fn classify(ext: &str) -> (AttachmentKind, &'static str) {
        match ext {
            "pdf" => (AttachmentKind::Pdf, "application/pdf"),
            "jpg" | "jpeg" => (AttachmentKind::Image, "image/jpeg"),
            "png" => (AttachmentKind::Image, "image/png"),
            "webp" => (AttachmentKind::Image, "image/webp"),
            "bmp" => (AttachmentKind::Image, "image/bmp"),
            "tiff" | "tif" => (AttachmentKind::Image, "image/tiff"),
            "svg" => (AttachmentKind::Image, "image/svg+xml"),
            "doc" | "docx" => (
                AttachmentKind::Document,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            "xls" | "xlsx" | "csv" => (
                AttachmentKind::Spreadsheet,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
            "dwg" | "dxf" => (AttachmentKind::CadDrawing, "application/acad"),
            "txt" | "md" | "json" | "yaml" | "toml" | "log" | "rs" | "py" | "c" | "cpp" | "h" => {
                (AttachmentKind::Text, "text/plain")
            }
            _ => (AttachmentKind::Binary, "application/octet-stream"),
        }
    }

    pub fn formatted_size(&self) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if self.size_bytes >= GB {
            format!("{:.2} GB", self.size_bytes as f64 / GB as f64)
        } else if self.size_bytes >= MB {
            format!("{:.1} MB", self.size_bytes as f64 / MB as f64)
        } else if self.size_bytes >= KB {
            format!("{:.0} KB", self.size_bytes as f64 / KB as f64)
        } else {
            format!("{} B", self.size_bytes)
        }
    }

    pub fn display_badge(&self) -> String {
        format!("[📎 {} ({})]", self.filename, self.formatted_size())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AttachmentState {
    pub pending: Vec<Attachment>,
}

impl AttachmentState {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub fn add(&mut self, attachment: Attachment) {
        if !self.pending.iter().any(|a| a.path == attachment.path) {
            self.pending.push(attachment);
        }
    }

    pub fn remove_at(&mut self, index: usize) -> Option<Attachment> {
        if index < self.pending.len() {
            Some(self.pending.remove(index))
        } else {
            None
        }
    }

    pub fn clear(&mut self) -> Vec<Attachment> {
        std::mem::take(&mut self.pending)
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_from_existing_file() {
        let attachment = Attachment::from_path("Cargo.toml").unwrap();
        assert_eq!(attachment.filename, "Cargo.toml");
        assert_eq!(attachment.extension, "toml");
        assert_eq!(attachment.kind, AttachmentKind::Text);
        assert!(attachment.size_bytes > 0);
    }

    #[test]
    fn test_attachment_not_found() {
        let err = Attachment::from_path("non_existent_file_12345.xyz").unwrap_err();
        match err {
            AttachmentError::NotFound(_) => (),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_attachment_size_formatting() {
        let mut att = Attachment::from_path("Cargo.toml").unwrap();
        att.size_bytes = 1024 * 1024 * 3 + 500_000;
        assert_eq!(att.formatted_size(), "3.5 MB");
    }
}
