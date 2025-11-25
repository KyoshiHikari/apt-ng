use anyhow::Result;
use crate::security::checks::{run_all_checks, SecurityCheckResult, Severity};

/// Security audit runner
pub struct SecurityAudit;

impl SecurityAudit {
    /// Run a complete security audit
    pub fn run() -> Result<SecurityAuditResult> {
        let checks = run_all_checks()?;
        
        let total_checks = checks.len();
        let passed_checks = checks.iter().filter(|c| c.passed).count();
        let failed_checks = total_checks - passed_checks;
        
        let critical_issues = checks.iter()
            .filter(|c| !c.passed && c.severity == Severity::Critical)
            .count();
        let high_issues = checks.iter()
            .filter(|c| !c.passed && c.severity == Severity::High)
            .count();
        let medium_issues = checks.iter()
            .filter(|c| !c.passed && c.severity == Severity::Medium)
            .count();
        
        Ok(SecurityAuditResult {
            checks,
            total_checks,
            passed_checks,
            failed_checks,
            critical_issues,
            high_issues,
            medium_issues,
        })
    }
}

/// Result of a security audit
#[derive(Debug)]
pub struct SecurityAuditResult {
    pub checks: Vec<SecurityCheckResult>,
    pub total_checks: usize,
    pub passed_checks: usize,
    pub failed_checks: usize,
    pub critical_issues: usize,
    pub high_issues: usize,
    pub medium_issues: usize,
}

impl SecurityAuditResult {
    /// Check if audit passed (no critical or high severity issues)
    pub fn passed(&self) -> bool {
        self.critical_issues == 0 && self.high_issues == 0
    }
    
    /// Get overall score (0-100)
    pub fn score(&self) -> u8 {
        if self.total_checks == 0 {
            return 100;
        }
        
        let base_score = (self.passed_checks as f64 / self.total_checks as f64) * 100.0;
        let penalty = (self.critical_issues as f64 * 20.0) + (self.high_issues as f64 * 10.0) + (self.medium_issues as f64 * 5.0);
        
        ((base_score - penalty).max(0.0).min(100.0)) as u8
    }
}

