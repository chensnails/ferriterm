//! Local system sampling for the monitor panel.

use std::time::Duration;

use serde::Serialize;
use sysinfo::{LoadAvg, System};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub cpu_usage: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub uptime_secs: u64,
}

pub struct Monitor {
    system: System,
}

impl Monitor {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        Self { system }
    }

    pub async fn sample(&mut self) -> MonitorSnapshot {
        // CPU usage is a delta between two refreshes.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        let load: LoadAvg = System::load_average();
        MonitorSnapshot {
            cpu_usage: self.system.global_cpu_usage(),
            mem_used: self.system.used_memory(),
            mem_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            load1: load.one,
            load5: load.five,
            load15: load.fifteen,
            uptime_secs: System::uptime(),
        }
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}
