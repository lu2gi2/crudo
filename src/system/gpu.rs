use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct GpuMonitor {
    // Cached availability to avoid unnecessary exec calls if binary is missing
    available: Option<bool>,
}

impl GpuMonitor {
    pub fn new() -> Self {
        Self { available: None }
    }

    /// Queries GPU utilization on Linux if an NVIDIA GPU and nvidia-smi are available.
    /// Returns None if no supported GPU or monitoring utility is found.
    pub fn query_utilization(&mut self) -> Option<u32> {
        // If we previously confirmed nvidia-smi does not exist, return None immediately
        if self.available == Some(false) {
            return None;
        }

        let output = match Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            Ok(out) => {
                self.available = Some(true);
                out
            }
            Err(_) => {
                self.available = Some(false);
                return None;
            }
        };

        if !output.status.success() {
            return None;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        // Take the first GPU line if multiple
        let first_line = text.lines().next()?.trim();
        first_line.parse::<u32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_monitor_returns_gracefully() {
        let mut monitor = GpuMonitor::new();
        // Either returns Some(util) or None, never panics
        let _ = monitor.query_utilization();
    }
}
