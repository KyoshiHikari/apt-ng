use clap::{Parser, Subcommand};

const HELP_TEMPLATE: &str = "\
{before-help}{about-with-newline}

{usage-heading}
  {usage}

{tab}Commands:
{subcommands}

{tab}Global Options:
{options}

{after-help}
";

#[derive(Parser)]
#[command(name = "apt-ng")]
#[command(about = "A modern, faster alternative to apt/apt-get")]
#[command(
    long_about = "apt-ng is a next-generation package manager written in Rust.\n\
    It provides faster package operations through parallelization, modern protocols,\n\
    and an efficient dependency solver.\n\n\
    Features:\n\
    • Parallel downloads and processing\n\
    • Modern .apx package format with zstd compression\n\
    • Ed25519 signature verification\n\
    • SQLite-based fast indexing\n\
    • Intelligent dependency resolution"
)]
#[command(
    help_template = HELP_TEMPLATE,
    after_help = "Examples:\n\
    \n\
    Update package index:\n\
      $ apt-ng update\n\
    \n\
    Search for packages:\n\
      $ apt-ng search nginx\n\
    \n\
    Install packages:\n\
      $ apt-ng install nginx curl\n\
    \n\
    Upgrade all packages:\n\
      $ apt-ng upgrade\n\
    \n\
    Show package information:\n\
      $ apt-ng show nginx\n\
    \n\
    For more information, visit: https://github.com/KyoshiHikari/apt-ng"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Number of parallel workers
    ///
    /// Controls how many packages can be downloaded/processed simultaneously.
    /// Default: CPU cores * 2
    #[arg(short = 'j', long = "jobs", global = true, value_name = "N")]
    pub jobs: Option<usize>,
    
    /// Show what would happen without executing
    ///
    /// Performs a dry run showing what actions would be taken without
    /// actually modifying the system.
    #[arg(long = "dry-run", global = true)]
    pub dry_run: bool,
    
    /// Verbose output
    ///
    /// Enables detailed output including dependency resolution steps,
    /// download progress, and installation details.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Update the local package index
    ///
    /// Downloads and updates package metadata from configured repositories.
    /// This command performs parallel downloads and signature verification.
    ///
    /// Examples:
    ///   $ apt-ng update
    ///   $ apt-ng update -v  # Verbose output
    #[command(alias = "up")]
    Update,
    
    /// Search for packages in the local index
    ///
    /// Searches the SQLite index for packages matching the given term.
    /// Results are displayed in a formatted table.
    ///
    /// Examples:
    ///   $ apt-ng search nginx
    ///   $ apt-ng search "web server"
    Search {
        /// Search term (package name or description)
        #[arg(value_name = "TERM")]
        term: String,
    },
    
    /// Install one or more packages
    ///
    /// Downloads and installs packages along with their dependencies.
    /// Uses parallel downloads and intelligent dependency resolution.
    ///
    /// Examples:
    ///   $ apt-ng install nginx
    ///   $ apt-ng install nginx curl -j 8  # Use 8 parallel workers
    ///   $ apt-ng install nginx --dry-run   # Preview installation
    #[command(alias = "i")]
    Install {
        /// Package name(s) to install
        #[arg(value_name = "PACKAGE", required = true)]
        packages: Vec<String>,
    },
    
    /// Remove one or more packages
    ///
    /// Removes installed packages from the system.
    /// Checks for dependencies before removal.
    ///
    /// Examples:
    ///   $ apt-ng remove nginx
    ///   $ apt-ng remove nginx curl
    #[command(alias = "rm")]
    Remove {
        /// Package name(s) to remove
        #[arg(value_name = "PACKAGE", required = true)]
        packages: Vec<String>,
    },
    
    /// Upgrade all installed packages
    ///
    /// Checks for available updates and upgrades all installed packages
    /// to their latest versions. Resolves dependencies automatically.
    ///
    /// Examples:
    ///   $ apt-ng upgrade
    ///   $ apt-ng upgrade --dry-run  # Preview upgrades
    Upgrade,
    
    /// Show detailed package information
    ///
    /// Displays comprehensive metadata about a package including
    /// version, dependencies, size, and description.
    ///
    /// Examples:
    ///   $ apt-ng show nginx
    ///   $ apt-ng show curl
    Show {
        /// Package name
        #[arg(value_name = "PACKAGE")]
        package: String,
    },
    
    /// Repository management
    ///
    /// Manage package repositories including adding new repositories
    /// and updating mirror priorities.
    #[command(subcommand)]
    Repo(RepoCommands),
    
    /// Cache management
    ///
    /// Manage the local package cache including cleaning and
    /// size management.
    #[command(subcommand)]
    Cache(CacheAction),
    
    /// Security audit
    ///
    /// Run security checks and generate security audit reports.
    ///
    /// Examples:
    ///   $ apt-ng security audit
    ///   $ apt-ng security audit --json
    #[command(subcommand)]
    Security(SecurityCommands),
    
    /// Update apt-ng to the latest version
    ///
    /// Checks GitHub Releases for newer versions and automatically
    /// downloads and installs the update if available.
    ///
    /// Examples:
    ///   $ apt-ng self-update
    ///   $ apt-ng self-update --force  # Force update even if same version
    SelfUpdate {
        /// Force update even if already on latest version
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum RepoCommands {
    /// Add a new repository
    ///
    /// Adds a new package repository to the configuration.
    /// The repository will be used for package updates and installations.
    ///
    /// Examples:
    ///   $ apt-ng repo add https://deb.debian.org/debian
    ///   $ apt-ng repo add https://mirror.example.com/debian
    Add {
        /// Repository URL
        #[arg(value_name = "URL")]
        url: String,
    },
    
    /// Probe mirrors and update prioritization
    ///
    /// Tests mirror performance (RTT and throughput) and updates
    /// repository priorities based on performance metrics.
    ///
    /// Examples:
    ///   $ apt-ng repo update
    ///   $ apt-ng repo update -v  # Verbose output
    Update,
    
    /// Generate repository index files
    ///
    /// Scans a directory for packages and generates Packages and Release files.
    ///
    /// Examples:
    ///   $ apt-ng repo generate /path/to/packages
    ///   $ apt-ng repo generate /path/to/packages --suite stable --component main
    Generate {
        /// Directory containing package files
        #[arg(value_name = "DIRECTORY")]
        directory: String,
        /// Suite name (e.g., stable, testing)
        #[arg(long, default_value = "stable")]
        suite: String,
        /// Component name (e.g., main, contrib)
        #[arg(long, default_value = "main")]
        component: String,
        /// Architecture (e.g., amd64, arm64)
        #[arg(long, default_value = "amd64")]
        arch: String,
        /// Signing key file (optional)
        #[arg(long)]
        key: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SecurityCommands {
    /// Run security audit
    ///
    /// Performs comprehensive security checks including signature verification,
    /// sandbox configuration, and input validation checks.
    Audit {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
pub enum CacheAction {
    /// Clean the package cache
    ///
    /// Removes cached packages. Can remove old versions or enforce
    /// size limits to free up disk space.
    ///
    /// Examples:
    ///   $ apt-ng cache clean                    # Remove all cached packages
    ///   $ apt-ng cache clean --old-versions      # Remove old versions only
    ///   $ apt-ng cache clean --max-size 1073741824  # Keep cache under 1GB
    Clean {
        /// Remove old package versions (keep only latest)
        ///
        /// For each package, removes all cached versions except the latest one.
        /// Useful for freeing space while keeping recent packages.
        #[arg(long = "old-versions")]
        old_versions: bool,
        
        /// Maximum cache size in bytes
        ///
        /// Removes oldest packages until cache size is below the specified limit.
        /// Examples: 1073741824 (1GB), 2147483648 (2GB)
        #[arg(long = "max-size", value_name = "BYTES")]
        max_size: Option<u64>,
    },
}

pub fn parse() -> Cli {
    Cli::parse()
}

/// Generate shell completion scripts
pub fn generate_completions(shell: &str, app: &mut clap::Command) {
    use clap_complete::{generate, shells};
    match shell {
        "zsh" => {
            generate(shells::Zsh, app, "apt-ng", &mut std::io::stdout());
        }
        "fish" => {
            generate(shells::Fish, app, "apt-ng", &mut std::io::stdout());
        }
        "bash" => {
            generate(shells::Bash, app, "apt-ng", &mut std::io::stdout());
        }
        "powershell" => {
            generate(shells::PowerShell, app, "apt-ng", &mut std::io::stdout());
        }
        _ => {
            eprintln!("Unsupported shell: {}", shell);
            eprintln!("Supported shells: zsh, fish, bash, powershell");
        }
    }
}

