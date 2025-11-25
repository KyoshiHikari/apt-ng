use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "apt-ng-benchmark")]
#[command(about = "Benchmark apt-ng against apt-get")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Benchmark update operation
    Update {
        /// Number of iterations
        #[arg(short, long, default_value_t = 3)]
        iterations: usize,
    },
    /// Benchmark install operation
    Install {
        /// Package(s) to install
        #[arg(required = true)]
        packages: Vec<String>,
        /// Number of iterations
        #[arg(short, long, default_value_t = 3)]
        iterations: usize,
    },
    /// Benchmark both update and install
    Full {
        /// Package(s) to install
        #[arg(required = true)]
        packages: Vec<String>,
        /// Number of iterations
        #[arg(short, long, default_value_t = 3)]
        iterations: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Update { iterations } => {
            apt_ng::benchmark::run_update_benchmark(iterations).await?;
        }
        Commands::Install { packages, iterations } => {
            apt_ng::benchmark::run_install_benchmark(&packages, iterations).await?;
        }
        Commands::Full { packages, iterations } => {
            apt_ng::benchmark::run_full_benchmark(&packages, iterations).await?;
        }
    }
    
    Ok(())
}

