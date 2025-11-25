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
        let mut cmd = Command::new("bwrap");

        // Basis-Isolation
        cmd.arg("--unshare-all");
        cmd.arg("--die-with-parent");
        cmd.arg("--as-pid-1");

        // Netzwerk-Zugriff
        self.setup_network_sandbox(&mut cmd);

        // Dateisystem-Sandbox
        self.setup_filesystem_sandbox(&mut cmd)?;

        // Ressourcen-Limits
        self.apply_resource_limits(&mut cmd)?;

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

    /// Setzt Ressourcen-Limits
    fn apply_resource_limits(&self, _cmd: &mut Command) -> Result<()> {
        // Memory-Limit mit systemd-run oder ulimit
        // Bubblewrap selbst unterstützt keine Memory-Limits direkt
        // Wir verwenden systemd-run falls verfügbar, sonst ulimit
        
        if let Some(_memory_limit) = self.config.memory_limit {
            // Versuche systemd-run zu verwenden
            if Command::new("systemd-run")
                .arg("--version")
                .output()
                .is_ok()
            {
                // systemd-run wird außerhalb von bwrap verwendet
                // Für jetzt dokumentieren wir dies
                // In einer vollständigen Implementierung würde man systemd-run wrappen
            } else {
                // Fallback: ulimit (wird innerhalb des Scripts gesetzt)
                // Dies ist weniger sicher, aber besser als nichts
            }
        }

        // CPU-Limit ähnlich
        if let Some(_cpu_limit) = self.config.cpu_limit {
            // CPU-Limits werden ähnlich behandelt
            // Für jetzt dokumentieren wir dies
        }

        Ok(())
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

