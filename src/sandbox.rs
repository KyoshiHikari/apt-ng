use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Konfiguration für Sandbox
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub network_allowed: bool,
    pub memory_limit: Option<u64>, // in Bytes
    pub cpu_limit: Option<f64>,     // z.B. 0.5 für 50%
    pub read_only_paths: Vec<String>,
    pub writable_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        SandboxConfig {
            enabled: true,
            network_allowed: false,
            memory_limit: Some(512 * 1024 * 1024), // 512 MB default
            cpu_limit: Some(1.0),                  // 100% CPU default
            read_only_paths: vec![],
            writable_paths: vec!["/tmp".to_string()],
        }
    }
}

/// Sandbox-Struktur für Hook-Ausführung
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    /// Erstellt eine neue Sandbox-Instanz
    #[allow(dead_code)]
    pub fn new(config: SandboxConfig) -> Self {
        Sandbox { config }
    }

    /// Prüft ob Bubblewrap verfügbar ist
    pub fn check_bubblewrap_available() -> bool {
        Command::new("bwrap")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Führt einen Hook in einer Sandbox aus
    pub fn execute_hook_sandboxed(
        &self,
        script_path: &Path,
        args: &[String],
        env_vars: &[(String, String)],
    ) -> Result<std::process::Output> {
        if !self.config.enabled {
            return Err(anyhow::anyhow!("Sandbox is disabled"));
        }

        if !Self::check_bubblewrap_available() {
            return Err(anyhow::anyhow!(
                "bubblewrap (bwrap) is not available. Please install it to use sandboxing."
            ));
        }

        let mut cmd = self.create_sandbox_command(script_path, args, env_vars)?;
        let output = cmd.output()?;
        Ok(output)
    }

    /// Erstellt einen bwrap-Command mit allen Limits
    fn create_sandbox_command(
        &self,
        script_path: &Path,
        args: &[String],
        env_vars: &[(String, String)],
    ) -> Result<Command> {
        // Check if we need to wrap with systemd-run for resource limits
        let needs_systemd_wrap = (self.config.memory_limit.is_some() || self.config.cpu_limit.is_some())
            && Self::check_systemd_run_available();

        let cmd = if needs_systemd_wrap {
            // Wrap bwrap with systemd-run for resource limits
            self.create_systemd_wrapped_command(script_path, args, env_vars)?
        } else {
            // Use bwrap directly
            self.create_bwrap_command(script_path, args, env_vars)?
        };

        Ok(cmd)
    }

    /// Creates a bwrap command without systemd-run wrapper
    fn create_bwrap_command(
        &self,
        script_path: &Path,
        args: &[String],
        env_vars: &[(String, String)],
    ) -> Result<Command> {
        let mut cmd = Command::new("bwrap");

        // Basis-Isolation
        cmd.arg("--unshare-all");
        cmd.arg("--die-with-parent");
        cmd.arg("--as-pid-1");

        // Netzwerk-Zugriff
        self.setup_network_sandbox(&mut cmd);

        // Dateisystem-Sandbox
        self.setup_filesystem_sandbox(&mut cmd)?;

        // Setze Umgebungsvariablen
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Führe Script aus
        cmd.arg("--");
        cmd.arg("/bin/sh");
        cmd.arg(script_path);
        for arg in args {
            cmd.arg(arg);
        }

        Ok(cmd)
    }

    /// Creates a systemd-run wrapped command with resource limits
    fn create_systemd_wrapped_command(
        &self,
        script_path: &Path,
        args: &[String],
        env_vars: &[(String, String)],
    ) -> Result<Command> {
        let mut systemd_cmd = Command::new("systemd-run");
        
        // Set memory limit if configured
        if let Some(memory_limit) = self.config.memory_limit {
            // Convert bytes to megabytes for systemd
            let memory_mb = (memory_limit / (1024 * 1024)).max(1);
            systemd_cmd.arg("--property=MemoryLimit");
            systemd_cmd.arg(format!("{}M", memory_mb));
        }

        // Set CPU limit if configured
        if let Some(cpu_limit) = self.config.cpu_limit {
            // CPUQuota expects percentage (e.g., "50%" for 50% of one CPU)
            let cpu_percent = (cpu_limit * 100.0) as u32;
            systemd_cmd.arg("--property=CPUQuota");
            systemd_cmd.arg(format!("{}%", cpu_percent));
        }

        // Set other systemd-run properties
        systemd_cmd.arg("--property=PrivateTmp=yes");
        systemd_cmd.arg("--property=ProtectSystem=strict");
        systemd_cmd.arg("--property=ProtectHome=yes");
        systemd_cmd.arg("--property=NoNewPrivileges=yes");

        // Build bwrap command arguments
        let mut bwrap_args = Vec::new();
        bwrap_args.push("--unshare-all".to_string());
        bwrap_args.push("--die-with-parent".to_string());
        bwrap_args.push("--as-pid-1".to_string());

        // Network sandbox
        if !self.config.network_allowed {
            bwrap_args.push("--unshare-net".to_string());
        }

        // Filesystem sandbox
        bwrap_args.push("--ro-bind".to_string());
        bwrap_args.push("/".to_string());
        bwrap_args.push("/".to_string());

        for path in &self.config.writable_paths {
            bwrap_args.push("--bind".to_string());
            bwrap_args.push(path.clone());
            bwrap_args.push(path.clone());
        }

        for path in &self.config.read_only_paths {
            bwrap_args.push("--ro-bind".to_string());
            bwrap_args.push(path.clone());
            bwrap_args.push(path.clone());
        }

        bwrap_args.push("--tmpfs".to_string());
        bwrap_args.push("/tmp".to_string());

        // Add bwrap command to systemd-run
        systemd_cmd.arg("--");
        systemd_cmd.arg("bwrap");
        for arg in bwrap_args {
            systemd_cmd.arg(arg);
        }
        systemd_cmd.arg("--");
        systemd_cmd.arg("/bin/sh");
        systemd_cmd.arg(script_path);
        for arg in args {
            systemd_cmd.arg(arg);
        }

        // Set environment variables
        for (key, value) in env_vars {
            systemd_cmd.env(key, value);
        }

        Ok(systemd_cmd)
    }

    /// Check if systemd-run is available
    fn check_systemd_run_available() -> bool {
        Command::new("systemd-run")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Konfiguriert Dateisystem-Sandbox
    fn setup_filesystem_sandbox(&self, cmd: &mut Command) -> Result<()> {
        // Root-Dateisystem als read-only binden
        cmd.arg("--ro-bind");
        cmd.arg("/");
        cmd.arg("/");

        // Writable paths
        for path in &self.config.writable_paths {
            cmd.arg("--bind");
            cmd.arg(path);
            cmd.arg(path);
        }

        // Read-only paths
        for path in &self.config.read_only_paths {
            cmd.arg("--ro-bind");
            cmd.arg(path);
            cmd.arg(path);
        }

        // Temp-Verzeichnis
        cmd.arg("--tmpfs");
        cmd.arg("/tmp");

        Ok(())
    }

    /// Setzt Ressourcen-Limits mit ulimit (Fallback wenn systemd-run nicht verfügbar)
    /// This is called when systemd-run is not available
    #[allow(dead_code)]
    fn apply_ulimit_limits(&self, _script_path: &Path) -> Result<String> {
        let mut ulimit_script = String::new();
        
        // Set memory limit with ulimit
        if let Some(memory_limit) = self.config.memory_limit {
            // ulimit -v sets virtual memory limit in KB
            let memory_kb = memory_limit / 1024;
            ulimit_script.push_str(&format!("ulimit -v {}; ", memory_kb));
        }

        // CPU limits with ulimit are less precise
        // We can use nice/renice, but it's not a hard limit
        if let Some(_cpu_limit) = self.config.cpu_limit {
            // Note: ulimit doesn't support CPU percentage limits directly
            // We could use cpulimit tool if available, but for now we skip it
            // as it requires additional dependencies
        }

        Ok(ulimit_script)
    }

    /// Setup Netzwerk-Sandbox
    #[allow(dead_code)]
    fn setup_network_sandbox(&self, cmd: &mut Command) {
        if !self.config.network_allowed {
            cmd.arg("--unshare-net");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(config.enabled);
        assert!(!config.network_allowed);
        assert!(config.memory_limit.is_some());
    }

    #[test]
    fn test_check_bubblewrap_available() {
        // Dies ist ein einfacher Test der prüft ob die Funktion aufgerufen werden kann
        let _ = Sandbox::check_bubblewrap_available();
    }
}

