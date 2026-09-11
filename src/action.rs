use std::path::PathBuf;

use crate::events::CrudoEvent;
use crate::system::SystemMetrics;

#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    Resize(u16, u16),
    Quit,
    Submit,
    ClearConversation,
    ShowHelp,
    ShowStatus,
    AttachFile(PathBuf),
    RemoveAttachment(usize),
    ToggleActivityDrawer,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToTop,
    ScrollToBottom,
    BackendEvent(CrudoEvent),
    SystemMetricsUpdate(SystemMetrics),
}
