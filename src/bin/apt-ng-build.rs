use clap::{Parser, Subcommand};
use anyhow::Result;
use std::path::{Path, PathBuf};
use apt_ng::apx_builder::{ApxBuilder, ApxSigner};
use apt_ng::package::PackageManifest;

#[derive(Parser)]
#[command(name = "apt-ng-build")]
#[command(about = "Build and sign .apx packages")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new .apx package from a directory
    Create {
        /// Source directory containing package files
        #[arg(short, long)]
        source: PathBuf,
        /// Output .apx file path
        #[arg(short, long)]
        output: PathBuf,
        /// Package name
        #[arg(short, long)]
        name: String,
        /// Package version
        #[arg(short, long)]
        version: String,
        /// Package architecture
        #[arg(short, long, default_value = "amd64")]
        arch: String,
    },
    /// Sign an .apx package
    Sign {
        /// Package file to sign
        #[arg(short, long)]
        package: PathBuf,
        /// Signing key file
        #[arg(short, long)]
        key: PathBuf,
        /// Output signature file (default: package.sig)
        #[arg(short, long)]
        signature: Option<PathBuf>,
    },
    /// Validate an .apx package
    Validate {
        /// Package file to validate
        #[arg(short, long)]
        package: PathBuf,
    },
    /// Generate a new signing key pair
    GenerateKey {
        /// Output directory for key files
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Key name prefix
        #[arg(short, long, default_value = "apt-ng")]
        name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create { source, output, name, version, arch } => {
            create_package(source, output, name, version, arch)?;
        }
        Commands::Sign { package, key, signature } => {
            sign_package(package, key, signature.as_deref())?;
        }
        Commands::Validate { package } => {
            validate_package(package)?;
        }
        Commands::GenerateKey { output, name } => {
            generate_key(output, name)?;
        }
    }

    Ok(())
}

fn create_package(
    source: &PathBuf,
    output: &PathBuf,
    name: &str,
    version: &str,
    arch: &str,
) -> Result<()> {
    println!("Creating package: {} ({}), arch: {}", name, version, arch);
    
    if !source.exists() {
        return Err(anyhow::anyhow!("Source directory does not exist: {:?}", source));
    }
    
    let mut builder = ApxBuilder::new(source);
    
    // Create manifest
    let manifest = PackageManifest {
        name: name.to_string(),
        version: version.to_string(),
        arch: arch.to_string(),
        provides: vec![],
        depends: vec![],
        conflicts: vec![],
        replaces: vec![],
        files: vec![],
        size: 0,
        checksum: String::new(),
        timestamp: 0,
        filename: None,
        repo_id: None,
    };
    
    builder.set_manifest(manifest);
    builder.build(output)?;
    
    println!("Package created: {:?}", output);
    Ok(())
}

fn sign_package(
    package: &PathBuf,
    key: &PathBuf,
    signature: Option<&Path>,
) -> Result<()> {
    println!("Signing package: {:?}", package);
    
    let signer = ApxSigner::from_key_file(key)?;
    
    let signature_path = signature.map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            let mut sig_path = package.clone();
            sig_path.set_extension("sig");
            sig_path
        });
    
    signer.sign_package(package, &signature_path)?;
    
    println!("Package signed: {:?}", signature_path);
    Ok(())
}

fn validate_package(package: &PathBuf) -> Result<()> {
    println!("Validating package: {:?}", package);
    
    // Try to open the package
    use apt_ng::package::ApxPackage;
    let _apx = ApxPackage::open(package)?;
    
    println!("Package is valid");
    Ok(())
}

fn generate_key(output: &PathBuf, name: &str) -> Result<()> {
    println!("Generating signing key pair...");
    
    let (signing_key, verifying_key) = ApxSigner::generate_key();
    
    // Write signing key (private)
    let signing_key_path = output.join(format!("{}.key", name));
    std::fs::write(&signing_key_path, signing_key.to_bytes())?;
    println!("Signing key written to: {:?}", signing_key_path);
    
    // Write verifying key (public)
    let verifying_key_path = output.join(format!("{}.pub", name));
    std::fs::write(&verifying_key_path, verifying_key.to_bytes())?;
    println!("Verifying key written to: {:?}", verifying_key_path);
    
    println!("Key pair generated successfully");
    Ok(())
}

