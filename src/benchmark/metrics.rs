use sysinfo::{System, Pid};
use std::time::Duration;
use std::thread;

/// Metriken-Struktur
#[derive(Debug, Clone)]
pub struct Metrics {
    pub cpu_usage: f32,
    pub memory_usage: u64, // in Bytes
    pub disk_read: u64,    // in Bytes
    pub disk_write: u64,   // in Bytes
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            cpu_usage: 0.0,
            memory_usage: 0,
            disk_read: 0,
            disk_write: 0,
        }
    }
}

/// Sammelt Metriken während der Ausführung einer Funktion
/// This function tracks metrics for a spawned process
pub fn collect_metrics<F, T>(f: F) -> anyhow::Result<Metrics>
where
    F: FnOnce() -> std::io::Result<T>,
{
    let mut system = System::new_all();
    system.refresh_all();
    
    // Get current process PID for baseline
    let current_pid = Pid::from(std::process::id() as usize);
    
    // Refresh system to get current process info
    system.refresh_process(current_pid);
    
    // Erfasse Baseline-Metriken für aktuellen Prozess
    let baseline_process = system.process(current_pid);
    let baseline_memory = baseline_process.map(|p| p.memory()).unwrap_or(0);
    let baseline_disk_read = baseline_process.map(|p| p.disk_usage().read_bytes).unwrap_or(0);
    let baseline_disk_write = baseline_process.map(|p| p.disk_usage().written_bytes).unwrap_or(0);
    let baseline_cpu = baseline_process.map(|p| p.cpu_usage()).unwrap_or(0.0);
    
    // Führe Funktion aus und track spawned process
    let result = f()?;
    
    // Wait a bit for process to start and collect metrics
    thread::sleep(Duration::from_millis(100));
    
    // Refresh system to get updated metrics
    system.refresh_process(current_pid);
    
    // Erfasse Metriken nach Ausführung
    let after_process = system.process(current_pid);
    let after_memory = after_process.map(|p| p.memory()).unwrap_or(0);
    let after_disk_read = after_process.map(|p| p.disk_usage().read_bytes).unwrap_or(0);
    let after_disk_write = after_process.map(|p| p.disk_usage().written_bytes).unwrap_or(0);
    let after_cpu = after_process.map(|p| p.cpu_usage()).unwrap_or(0.0);
    
    // Calculate deltas
    let memory_delta = after_memory.saturating_sub(baseline_memory);
    let disk_read_delta = after_disk_read.saturating_sub(baseline_disk_read);
    let disk_write_delta = after_disk_write.saturating_sub(baseline_disk_write);
    let cpu_delta = (after_cpu - baseline_cpu).max(0.0);
    
    // Drop result to avoid unused warning
    let _ = result;
    
    Ok(Metrics {
        cpu_usage: cpu_delta,
        memory_usage: memory_delta,
        disk_read: disk_read_delta,
        disk_write: disk_write_delta,
    })
}

