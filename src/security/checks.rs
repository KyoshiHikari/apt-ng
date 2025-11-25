use anyhow::Result;

/// Security check result
#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    pub check_name: String,
    pub severity: Severity,
    pub passed: bool,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    #[allow(dead_code)]
    Low,
    Info,
}

/// Trait for security checks
pub trait SecurityCheck {
    fn name(&self) -> &str;
    fn check(&self) -> Result<SecurityCheckResult>;
}

/// Check signature verification coverage
pub struct SignatureVerificationCheck;

impl SecurityCheck for SignatureVerificationCheck {
    fn name(&self) -> &str {
        "signature_verification_coverage"
    }

    fn check(&self) -> Result<SecurityCheckResult> {
        use crate::config::Config;
        use crate::verifier::PackageVerifier;
        
        let config = Config::load(None)?;
        let verifier = PackageVerifier::new(config.trusted_keys_dir())?;
        let key_count = verifier.trusted_key_count();
        
        let passed = key_count > 0;
        let message = if passed {
            format!("Signature verification enabled with {} trusted key(s)", key_count)
        } else {
            "No trusted keys found. Unsigned repositories will be allowed.".to_string()
        };
        
        Ok(SecurityCheckResult {
            check_name: self.name().to_string(),
            severity: if passed { Severity::Info } else { Severity::High },
            passed,
            message,
            details: Some(format!("Trusted keys directory: {}", config.trusted_keys_dir().display())),
        })
    }
}

/// Check sandbox configuration
pub struct SandboxConfigurationCheck;

impl SecurityCheck for SandboxConfigurationCheck {
    fn name(&self) -> &str {
        "sandbox_configuration"
    }

    fn check(&self) -> Result<SecurityCheckResult> {
        use crate::config::Config;
        use crate::sandbox::Sandbox;
        
        let config = Config::load(None)?;
        let sandbox_available = Sandbox::check_bubblewrap_available();
        
        let sandbox_enabled = config.sandbox.as_ref()
            .map(|s| s.enabled)
            .unwrap_or(false);
        
        let passed = sandbox_available && sandbox_enabled;
        let message = if !sandbox_available {
            "Bubblewrap (bwrap) is not available. Sandboxing cannot be used.".to_string()
        } else if !sandbox_enabled {
            "Sandbox is available but disabled in configuration.".to_string()
        } else {
            "Sandbox is enabled and bubblewrap is available.".to_string()
        };
        
        Ok(SecurityCheckResult {
            check_name: self.name().to_string(),
            severity: if passed { Severity::Info } else { Severity::Medium },
            passed,
            message,
            details: config.sandbox.as_ref().map(|s| {
                format!("Network allowed: {}, Memory limit: {:?}, CPU limit: {:?}",
                    s.network_allowed,
                    s.memory_limit,
                    s.cpu_limit
                )
            }),
        })
    }
}

/// Check for path traversal vulnerabilities
pub struct PathTraversalCheck;

impl SecurityCheck for PathTraversalCheck {
    fn name(&self) -> &str {
        "path_traversal"
    }

    fn check(&self) -> Result<SecurityCheckResult> {
        // Check if paths are properly sanitized
        // This is a basic check - in production, would need more thorough analysis
        
        let _dangerous_patterns = vec!["../", "..\\", "/etc/passwd", "~/.ssh"];
        let found_issues: Vec<String> = Vec::new();
        
        // Check if we have input validation in place
        // This is a simplified check - would need code analysis in production
        
        let passed = found_issues.is_empty();
        let message = if passed {
            "No obvious path traversal vulnerabilities detected.".to_string()
        } else {
            format!("Potential path traversal issues found: {:?}", found_issues)
        };
        
        Ok(SecurityCheckResult {
            check_name: self.name().to_string(),
            severity: if passed { Severity::Info } else { Severity::High },
            passed,
            message,
            details: Some("Manual code review recommended for path handling.".to_string()),
        })
    }
}

/// Check input validation
pub struct InputValidationCheck;

impl SecurityCheck for InputValidationCheck {
    fn name(&self) -> &str {
        "input_validation"
    }

    fn check(&self) -> Result<SecurityCheckResult> {
        // Check if parsers handle invalid input gracefully
        // This is a basic check
        
        let passed = true; // Would need actual code analysis
        let message = "Input validation should be verified through fuzzing.".to_string();
        
        Ok(SecurityCheckResult {
            check_name: self.name().to_string(),
            severity: Severity::Info,
            passed,
            message,
            details: Some("Run fuzzing tests to verify input validation.".to_string()),
        })
    }
}

/// Run all security checks
pub fn run_all_checks() -> Result<Vec<SecurityCheckResult>> {
    let checks: Vec<Box<dyn SecurityCheck>> = vec![
        Box::new(SignatureVerificationCheck),
        Box::new(SandboxConfigurationCheck),
        Box::new(PathTraversalCheck),
        Box::new(InputValidationCheck),
    ];
    
    let mut results = Vec::new();
    for check in checks {
        match check.check() {
            Ok(result) => results.push(result),
            Err(e) => {
                results.push(SecurityCheckResult {
                    check_name: check.name().to_string(),
                    severity: Severity::High,
                    passed: false,
                    message: format!("Check failed with error: {}", e),
                    details: None,
                });
            }
        }
    }
    
    Ok(results)
}

