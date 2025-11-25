use sysinfo::System;

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
pub fn collect_metrics<F, T>(f: F) -> anyhow::Result<Metrics>
where
    F: FnOnce() -> std::io::Result<T>,
{
    let mut system = System::new_all();
    system.refresh_all();
    
    // Erfasse Baseline-Metriken
    let baseline_memory = system.used_memory();
    
    // Führe Funktion aus
    let _result = f()?;
    
    // Erfasse Metriken nach Ausführung
    system.refresh_all();
    let after_memory = system.used_memory();
    
    // CPU-Auslastung ist schwierig genau zu messen ohne Process-Tracking
    // Für jetzt verwenden wir einen einfachen Ansatz
    let cpu_usage = 0.0; // Placeholder
    
    Ok(Metrics {
        cpu_usage,
        memory_usage: after_memory.saturating_sub(baseline_memory),
        disk_read: 0,  // Disk-I/O ist schwierig zu messen ohne spezielle Tools
        disk_write: 0,
    })
}

