pub mod metrics;
pub mod output;

use crate::benchmark::metrics::{Metrics, collect_metrics};
use crate::benchmark::output::format_results;
use std::process::Command;
use std::time::Instant;

/// Benchmark-Ergebnisse
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub tool: String,
    pub operation: String,
    pub duration: std::time::Duration,
    pub metrics: Metrics,
}

/// Führt Update-Benchmark aus
pub async fn run_update_benchmark(iterations: usize) -> anyhow::Result<()> {
    println!("Running update benchmark ({} iterations)...", iterations);
    
    let mut apt_get_results = Vec::new();
    let mut apt_ng_results = Vec::new();
    
    for i in 1..=iterations {
        println!("\nIteration {}/{}", i, iterations);
        
        // Benchmark apt-get update
        println!("Benchmarking apt-get update...");
        let start = Instant::now();
        let apt_get_metrics = collect_metrics(|| {
            Command::new("apt-get")
                .arg("update")
                .output()
        })?;
        let apt_get_duration = start.elapsed();
        
        apt_get_results.push(BenchmarkResult {
            tool: "apt-get".to_string(),
            operation: "update".to_string(),
            duration: apt_get_duration,
            metrics: apt_get_metrics,
        });
        
        // Benchmark apt-ng update
        println!("Benchmarking apt-ng update...");
        let start = Instant::now();
        let apt_ng_metrics = collect_metrics(|| {
            Command::new("apt-ng")
                .arg("update")
                .output()
        })?;
        let apt_ng_duration = start.elapsed();
        
        apt_ng_results.push(BenchmarkResult {
            tool: "apt-ng".to_string(),
            operation: "update".to_string(),
            duration: apt_ng_duration,
            metrics: apt_ng_metrics,
        });
    }
    
    format_results(&apt_get_results, &apt_ng_results);
    
    Ok(())
}

/// Führt Install-Benchmark aus
pub async fn run_install_benchmark(packages: &[String], iterations: usize) -> anyhow::Result<()> {
    println!("Running install benchmark for {:?} ({} iterations)...", packages, iterations);
    
    let mut apt_get_results = Vec::new();
    let mut apt_ng_results = Vec::new();
    
    for i in 1..=iterations {
        println!("\nIteration {}/{}", i, iterations);
        
        // Benchmark apt-get install
        println!("Benchmarking apt-get install...");
        let start = Instant::now();
        let apt_get_metrics = collect_metrics(|| {
            let mut cmd = Command::new("apt-get");
            cmd.arg("install").arg("-y");
            for pkg in packages {
                cmd.arg(pkg);
            }
            cmd.output()
        })?;
        let apt_get_duration = start.elapsed();
        
        apt_get_results.push(BenchmarkResult {
            tool: "apt-get".to_string(),
            operation: "install".to_string(),
            duration: apt_get_duration,
            metrics: apt_get_metrics,
        });
        
        // Benchmark apt-ng install
        println!("Benchmarking apt-ng install...");
        let start = Instant::now();
        let apt_ng_metrics = collect_metrics(|| {
            let mut cmd = Command::new("apt-ng");
            cmd.arg("install");
            for pkg in packages {
                cmd.arg(pkg);
            }
            cmd.output()
        })?;
        let apt_ng_duration = start.elapsed();
        
        apt_ng_results.push(BenchmarkResult {
            tool: "apt-ng".to_string(),
            operation: "install".to_string(),
            duration: apt_ng_duration,
            metrics: apt_ng_metrics,
        });
    }
    
    format_results(&apt_get_results, &apt_ng_results);
    
    Ok(())
}

/// Führt vollständigen Benchmark aus (update + install)
pub async fn run_full_benchmark(packages: &[String], iterations: usize) -> anyhow::Result<()> {
    println!("Running full benchmark (update + install) for {:?} ({} iterations)...", packages, iterations);
    
    // Führe Update-Benchmark aus
    run_update_benchmark(iterations).await?;
    
    // Führe Install-Benchmark aus
    run_install_benchmark(packages, iterations).await?;
    
    Ok(())
}

