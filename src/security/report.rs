use crate::security::audit::SecurityAuditResult;
use crate::security::checks::Severity;
use serde::Serialize;

/// Security report generator
pub struct SecurityReport;

impl SecurityReport {
    /// Generate a text report
    pub fn generate_text(result: &SecurityAuditResult) -> String {
        let mut output = String::new();
        
        output.push_str("=== Security Audit Report ===\n\n");
        output.push_str(&format!("Total Checks: {}\n", result.total_checks));
        output.push_str(&format!("Passed: {}\n", result.passed_checks));
        output.push_str(&format!("Failed: {}\n", result.failed_checks));
        output.push_str(&format!("Score: {}%\n\n", result.score()));
        
        if result.critical_issues > 0 {
            output.push_str(&format!("⚠️  Critical Issues: {}\n", result.critical_issues));
        }
        if result.high_issues > 0 {
            output.push_str(&format!("⚠️  High Issues: {}\n", result.high_issues));
        }
        if result.medium_issues > 0 {
            output.push_str(&format!("⚠️  Medium Issues: {}\n", result.medium_issues));
        }
        
        output.push_str("\n=== Detailed Results ===\n\n");
        
        for check in &result.checks {
            let status = if check.passed { "✓" } else { "✗" };
            let severity_icon = match check.severity {
                Severity::Critical => "🔴",
                Severity::High => "🟠",
                Severity::Medium => "🟡",
                Severity::Low => "🟢",
                Severity::Info => "ℹ️",
            };
            
            output.push_str(&format!("{} {} {}\n", status, severity_icon, check.check_name));
            output.push_str(&format!("  {}\n", check.message));
            if let Some(ref details) = check.details {
                output.push_str(&format!("  Details: {}\n", details));
            }
            output.push_str("\n");
        }
        
        output
    }
    
    /// Generate a JSON report
    pub fn generate_json(result: &SecurityAuditResult) -> anyhow::Result<String> {
        #[derive(Serialize)]
        struct CheckResultJson {
            check_name: String,
            severity: String,
            passed: bool,
            message: String,
            details: Option<String>,
        }
        
        #[derive(Serialize)]
        struct AuditResultJson {
            total_checks: usize,
            passed_checks: usize,
            failed_checks: usize,
            score: u8,
            critical_issues: usize,
            high_issues: usize,
            medium_issues: usize,
            checks: Vec<CheckResultJson>,
        }
        
        let checks_json: Vec<CheckResultJson> = result.checks.iter().map(|c| {
            CheckResultJson {
                check_name: c.check_name.clone(),
                severity: format!("{:?}", c.severity),
                passed: c.passed,
                message: c.message.clone(),
                details: c.details.clone(),
            }
        }).collect::<Vec<CheckResultJson>>();
        
        let audit_json = AuditResultJson {
            total_checks: result.total_checks,
            passed_checks: result.passed_checks,
            failed_checks: result.failed_checks,
            score: result.score(),
            critical_issues: result.critical_issues,
            high_issues: result.high_issues,
            medium_issues: result.medium_issues,
            checks: checks_json,
        };
        
        Ok(serde_json::to_string_pretty(&audit_json)?)
    }
}

