use sysinfo::System;

pub struct CpuMonitor {
    system: System,
    initialized: bool,
}

impl Default for CpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuMonitor {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        Self {
            system,
            initialized: false,
        }
    }

    /// Refreshes and returns the current global CPU usage percentage and core count.
    /// In sysinfo, the first refresh initializes baselines, subsequent refreshes compute the delta.
    pub fn refresh(&mut self) -> (f32, usize) {
        self.system.refresh_cpu_all();
        let usage = self.system.global_cpu_usage();
        let cores = self.system.cpus().len();
        self.initialized = true;
        (usage, cores)
    }

    pub fn core_count(&self) -> usize {
        self.system.cpus().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_monitor() {
        let mut monitor = CpuMonitor::new();
        let (usage, cores) = monitor.refresh();
        assert!(cores > 0, "System should have at least 1 core");
        assert!(usage >= 0.0, "CPU usage should be non-negative");
    }
}
