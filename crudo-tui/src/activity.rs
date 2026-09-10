pub const DEFAULT_MAX_ACTIVITIES: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ActivityStatus {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            ActivityStatus::Pending => "pending",
            ActivityStatus::Running => "running",
            ActivityStatus::Completed => "completed",
            ActivityStatus::Failed => "failed",
            ActivityStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Activity {
    pub id: usize,
    pub tool: String,
    pub status: ActivityStatus,
    pub summary: String,
    pub progress: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
}

impl Activity {
    pub fn new(id: usize, tool: String, summary: String) -> Self {
        Self {
            id,
            tool,
            status: ActivityStatus::Pending,
            summary,
            progress: None,
            error: None,
            duration_ms: None,
        }
    }

    /// Returns a clean, concise tool name for columnar display (e.g. "search_files", "read_file").
    pub fn display_name(&self) -> String {
        let mut name = if self.tool.is_empty() {
            "tool".to_string()
        } else {
            self.tool.clone()
        };

        if let Some(stripped) = name.strip_prefix("Running ") {
            name = stripped.to_string();
        } else if let Some(stripped) = name.strip_prefix("running ") {
            name = stripped.to_string();
        }
        name.trim_end_matches('.').to_string()
    }

    /// Returns a user-friendly status or duration string (e.g. "1.2s", "running", "failed", "cancelled").
    pub fn status_text(&self) -> String {
        match self.status {
            ActivityStatus::Running => "running".to_string(),
            ActivityStatus::Completed => {
                if let Some(ms) = self.duration_ms {
                    format!("{:.1}s", ms as f64 / 1000.0)
                } else {
                    "completed".to_string()
                }
            }
            ActivityStatus::Failed => "failed".to_string(),
            ActivityStatus::Cancelled => "cancelled".to_string(),
            ActivityStatus::Pending => "pending".to_string(),
        }
    }

    /// Returns a concise, user-friendly display title for the activity.
    /// Cleans up internal prefixes/suffixes when finished (e.g., "Running cargo check..." -> "cargo check").
    #[allow(dead_code)]
    pub fn display_title(&self) -> String {
        let tool_clean = if self.tool.eq_ignore_ascii_case("read file") {
            "Read"
        } else {
            self.tool.as_str()
        };

        let base = if self.summary.is_empty() {
            tool_clean.to_string()
        } else if tool_clean.is_empty() {
            self.summary.clone()
        } else {
            format!("{} {}", tool_clean, self.summary)
        };

        match self.status {
            ActivityStatus::Running => base,
            _ => {
                // Strip "Running " and trailing "..." once finished/cancelled/failed
                let mut cleaned = base.as_str();
                if let Some(stripped) = cleaned.strip_prefix("Running ") {
                    cleaned = stripped;
                } else if let Some(stripped) = cleaned.strip_prefix("running ") {
                    cleaned = stripped;
                }
                cleaned.trim_end_matches('.').to_string()
            }
        }
    }

    /// Updates concise progress from tool output, bounded to max 80 characters.
    /// Extracts the last non-empty line or summarizes lines without unbounded memory dump.
    pub fn update_progress(&mut self, output: &str) {
        let trimmed_output = output.trim();
        if trimmed_output.is_empty() {
            self.progress = Some("output received".to_string());
            return;
        }

        // If error prefix detected, store in error field
        if trimmed_output.starts_with("Error:") || trimmed_output.starts_with("error:") {
            let err_clean = trimmed_output
                .strip_prefix("Error:")
                .or_else(|| trimmed_output.strip_prefix("error:"))
                .unwrap_or(trimmed_output)
                .trim();
            self.error = Some(err_clean.to_string());
            return;
        }

        if let Some(last_line) = output.lines().rev().find(|l| !l.trim().is_empty()) {
            let trimmed = last_line.trim();
            let concise = if trimmed.chars().count() > 80 {
                let mut truncated: String = trimmed.chars().take(77).collect();
                truncated.push_str("...");
                truncated
            } else {
                trimmed.to_string()
            };
            self.progress = Some(concise);
        } else {
            self.progress = Some("output received".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_lifecycle_status_and_text() {
        let mut act = Activity::new(1, "search_files".to_string(), "*.rs".to_string());
        assert_eq!(act.status, ActivityStatus::Pending);
        assert_eq!(act.status_text(), "pending");
        assert_eq!(act.display_name(), "search_files");

        act.status = ActivityStatus::Running;
        assert_eq!(act.status_text(), "running");

        act.status = ActivityStatus::Completed;
        act.duration_ms = Some(1200);
        assert_eq!(act.status_text(), "1.2s");

        act.status = ActivityStatus::Failed;
        assert_eq!(act.status_text(), "failed");

        act.status = ActivityStatus::Cancelled;
        assert_eq!(act.status_text(), "cancelled");
    }

    #[test]
    fn test_activity_progress_and_error_extraction() {
        let mut act = Activity::new(1, "execute_command".to_string(), "".to_string());

        // Normal output
        act.update_progress("Line 1\nLine 2\nDone building");
        assert_eq!(act.progress.as_deref(), Some("Done building"));
        assert!(act.error.is_none());

        // Error output
        act.update_progress("Error: Permission denied");
        assert_eq!(act.error.as_deref(), Some("Permission denied"));

        // Empty output
        let mut empty_act = Activity::new(2, "read_file".to_string(), "".to_string());
        empty_act.update_progress("   ");
        assert_eq!(empty_act.progress.as_deref(), Some("output received"));
    }
}
