pub mod cpu;
pub mod gpu;
pub mod time;

pub use cpu::CpuMonitor;
pub use gpu::GpuMonitor;
pub use time::{current_system_date, current_system_time, current_system_time_formatted};

#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub cpu_usage: Option<f32>,
    pub cpu_cores: usize,
    pub gpu_usage: Option<u32>,
}

impl SystemMetrics {
    pub fn formatted_cpu(&self) -> String {
        match self.cpu_usage {
            Some(usage) => {
                if self.cpu_cores > 0 {
                    format!("{:.0}% / {} cores", usage, self.cpu_cores)
                } else {
                    format!("{:.0}%", usage)
                }
            }
            None => "CALC...".to_string(),
        }
    }

    pub fn formatted_cpu_percent(&self) -> String {
        match self.cpu_usage {
            Some(usage) => format!("{:.0}%", usage),
            None => "N/A".to_string(),
        }
    }

    pub fn formatted_cpu_compact(&self) -> String {
        match self.cpu_usage {
            Some(usage) => format!("{:.0}%", usage),
            None => "--%".to_string(),
        }
    }

    pub fn formatted_gpu(&self) -> String {
        match self.gpu_usage {
            Some(usage) => format!("{}%", usage),
            None => "N/A".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_formatting() {
        let metrics = SystemMetrics {
            cpu_usage: Some(18.4),
            cpu_cores: 16,
            gpu_usage: Some(34),
        };
        assert_eq!(metrics.formatted_cpu(), "18% / 16 cores");
        assert_eq!(metrics.formatted_cpu_compact(), "18%");
        assert_eq!(metrics.formatted_gpu(), "34%");

        let no_gpu = SystemMetrics {
            cpu_usage: Some(5.2),
            cpu_cores: 8,
            gpu_usage: None,
        };
        assert_eq!(no_gpu.formatted_gpu(), "N/A");
    }
}
