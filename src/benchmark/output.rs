use crate::benchmark::BenchmarkResult;
use comfy_table::Table;

/// Formatiert Benchmark-Ergebnisse
pub fn format_results(apt_get_results: &[BenchmarkResult], apt_ng_results: &[BenchmarkResult]) {
    println!("\n{}", "=".repeat(80));
    println!("Benchmark Results");
    println!("{}", "=".repeat(80));
    
    // Berechne Durchschnitte
    let apt_get_avg = calculate_average(apt_get_results);
    let apt_ng_avg = calculate_average(apt_ng_results);
    
    // Erstelle Vergleichstabelle
    let mut table = Table::new();
    table.set_header(vec!["Metric", "apt-get", "apt-ng", "Difference", "Speedup"]);
    
    // Duration
    let duration_diff = apt_get_avg.duration.as_secs_f64() - apt_ng_avg.duration.as_secs_f64();
    let speedup = if apt_ng_avg.duration.as_secs_f64() > 0.0 {
        apt_get_avg.duration.as_secs_f64() / apt_ng_avg.duration.as_secs_f64()
    } else {
        0.0
    };
    
    table.add_row(vec![
        "Duration (s)",
        &format!("{:.2}", apt_get_avg.duration.as_secs_f64()),
        &format!("{:.2}", apt_ng_avg.duration.as_secs_f64()),
        &format!("{:.2}", duration_diff),
        &format!("{:.2}x", speedup),
    ]);
    
    // CPU Usage
    let cpu_diff = apt_get_avg.metrics.cpu_usage - apt_ng_avg.metrics.cpu_usage;
    table.add_row(vec![
        "CPU Usage (%)",
        &format!("{:.2}", apt_get_avg.metrics.cpu_usage),
        &format!("{:.2}", apt_ng_avg.metrics.cpu_usage),
        &format!("{:.2}", cpu_diff),
        "-",
    ]);
    
    // Memory Usage
    let memory_diff = apt_get_avg.metrics.memory_usage as i64 - apt_ng_avg.metrics.memory_usage as i64;
    table.add_row(vec![
        "Memory Usage (MB)",
        &format!("{:.2}", apt_get_avg.metrics.memory_usage as f64 / 1024.0 / 1024.0),
        &format!("{:.2}", apt_ng_avg.metrics.memory_usage as f64 / 1024.0 / 1024.0),
        &format!("{:.2}", memory_diff as f64 / 1024.0 / 1024.0),
        "-",
    ]);
    
    println!("\n{}", table);
    
    // Detaillierte Ergebnisse pro Iteration
    println!("\n{}", "=".repeat(80));
    println!("Detailed Results per Iteration");
    println!("{}", "=".repeat(80));
    
    for (i, (apt_get, apt_ng)) in apt_get_results.iter().zip(apt_ng_results.iter()).enumerate() {
        println!("\nIteration {}:", i + 1);
        println!("  apt-get: {:.2}s, CPU: {:.2}%, Memory: {:.2} MB",
            apt_get.duration.as_secs_f64(),
            apt_get.metrics.cpu_usage,
            apt_get.metrics.memory_usage as f64 / 1024.0 / 1024.0
        );
        println!("  apt-ng:  {:.2}s, CPU: {:.2}%, Memory: {:.2} MB",
            apt_ng.duration.as_secs_f64(),
            apt_ng.metrics.cpu_usage,
            apt_ng.metrics.memory_usage as f64 / 1024.0 / 1024.0
        );
    }
}

/// Berechnet Durchschnittswerte
fn calculate_average(results: &[BenchmarkResult]) -> BenchmarkResult {
    if results.is_empty() {
        return BenchmarkResult {
            tool: "average".to_string(),
            operation: "average".to_string(),
            duration: std::time::Duration::from_secs(0),
            metrics: Default::default(),
        };
    }
    
    let total_duration: u64 = results.iter()
        .map(|r| r.duration.as_nanos() as u64)
        .sum();
    let avg_duration = std::time::Duration::from_nanos(total_duration / results.len() as u64);
    
    let avg_cpu: f32 = results.iter()
        .map(|r| r.metrics.cpu_usage)
        .sum::<f32>() / results.len() as f32;
    
    let avg_memory: u64 = results.iter()
        .map(|r| r.metrics.memory_usage)
        .sum::<u64>() / results.len() as u64;
    
    BenchmarkResult {
        tool: "average".to_string(),
        operation: "average".to_string(),
        duration: avg_duration,
        metrics: crate::benchmark::metrics::Metrics {
            cpu_usage: avg_cpu,
            memory_usage: avg_memory,
            disk_read: 0,
            disk_write: 0,
        },
    }
}

