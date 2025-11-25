# apt-ng

> **apt-ng** (apt Next Generation) – A modern, faster alternative to `apt` / `apt-get`, implemented in Rust.

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## 🚀 Features

- **Faster**: Multi-threaded, IO-parallelized downloading and decompression
- **Modern**: Rust implementation with modern protocols (HTTP/2, zstd compression)
- **Secure**: Ed25519-based signature verification
- **Efficient**: SQLite-based metadata index for fast searches
- **Compatible**: Supports Debian/APT repositories
- **User-friendly**: Colored output with progress indicators

## 📦 Installation

### Quick Install (Recommended)

Install with a single command:

```bash
/bin/bash -c "$(curl -sL https://raw.githubusercontent.com/KyoshiHikari/apt-ng/main/quick-install)"
```

This will:
- Download pre-built binary from GitHub Releases (if available for your architecture)
- If no pre-built binary is available, it will:
  - Install Rust/Cargo if needed
  - Clone the repository
  - Build apt-ng from source
- Install it to `/usr/local/bin/apt-ng`
- Install shell completions

**Note**: Pre-built binaries are available for common architectures (x86_64, aarch64). For other architectures or if no release is available, the script will automatically build from source.

### Manual Build

If you prefer to build manually:

```bash
git clone https://github.com/KyoshiHikari/apt-ng.git
cd apt-ng
cargo build --release
sudo cp target/release/apt-ng /usr/local/bin/
```

### Prerequisites

- Rust 1.70 or higher (installed automatically by install script)
- Cargo (Rust Package Manager)

## 🎯 Usage

### Basic Commands

```bash
# Update package index
apt-ng update

# Search for a package
apt-ng search <package-name>

# Install a package
apt-ng install <package-name>

# Show package information
apt-ng show <package-name>

# Remove a package
apt-ng remove <package-name>

# Add a repository
apt-ng repo add <url>

# Clean cache
apt-ng cache clean
```

### Options

- `-j, --jobs N`: Number of parallel workers (Default: CPU * 2)
- `--dry-run`: Show what would happen without executing
- `-v, --verbose`: Verbose output

### Examples

```bash
# Update with 8 parallel jobs
apt-ng update -j 8

# Dry-run for installation
apt-ng install micro --dry-run

# Verbose output
apt-ng install micro -v
```

## 🏗️ Architecture

```
CLI -> Core Engine ->
  - Index (SQLite)
  - Downloader (HTTP/2, parallel)
  - Verifier (Ed25519)
  - Solver (Dependency Resolution)
  - Installer (Worker-Pool)
  - Cache Manager
```

## 📋 Supported Formats

- **Packages files**: `.gz`, `.xz` (compressed) and uncompressed
- **Packages**: `.deb` (Debian packages) and `.apx` (custom package format with zstd compression)
- **Signatures**: Ed25519-based signatures for repositories and packages

## 🔧 Development

### Setting up Git Authentication

For automatic pushing to GitHub, see [docs/GIT-AUTH.md](docs/GIT-AUTH.md).

Quick setup:
```bash
# 1. Create .env from .env.example
cp .env.example .env
# 2. Add your GitHub Personal Access Token to .env
# 3. Set up authentication
./scripts/setup-git-auth.sh
```

### Project Structure

```
apt-ng/
├── src/
│   ├── main.rs          # CLI Entry Point
│   ├── cli.rs           # CLI Parsing
│   ├── config.rs        # Configuration Management
│   ├── index.rs         # SQLite Index
│   ├── downloader.rs    # HTTP Downloader
│   ├── verifier.rs      # Signature Verification
│   ├── installer.rs     # Package Installation
│   ├── package.rs       # Package Format Handling
│   ├── repo.rs          # Repository Management
│   ├── solver.rs        # Dependency Solver
│   ├── cache.rs         # Cache Management
│   ├── apt_parser.rs    # APT Packages Parser
│   ├── system.rs        # System Detection
│   └── output.rs        # Formatted Output
├── docs/
│   ├── INSTRUCTION.md   # Technical Design
│   └── FUNCTIONS-LIST.md # Feature Status
└── Cargo.toml
```

### Running Tests

```bash
cargo test
```

## 🛣️ Roadmap

See [FUNCTIONS-LIST.md](docs/FUNCTIONS-LIST.md) for the current implementation status.

### Implemented Features ✅

- [x] Full Dependency Solver with version constraints and conflict detection
- [x] Atomic Moves for Installations with rollback support
- [x] Rollback Mechanism for failed installations
- [x] Range-Requests for Chunk Downloads
- [x] Resume capability for interrupted downloads
- [x] .apx Package Format Support with signature verification
- [x] Repository and Package Signature Verification (Ed25519)
- [x] Checksum validation during downloads and extraction
- [x] Pre/post install hooks support

### Planned Features

- [ ] Integration Tests with local test repository
- [ ] Benchmarking tools against apt-get
- [ ] Fuzzing for package format parsers
- [ ] Security analysis (Signatures & Hook Sandbox)

## 🤝 Contributing

Contributions are welcome! Please create an Issue or Pull Request.

## 📄 License

This project is licensed under the MIT License.

## 🙏 Acknowledgments

- Inspired by `apt` and `apt-get`
- Uses modern Rust crates for performance and security

## 📝 What does "ng" mean?

**ng** stands for **Next Generation** – a modern, improved version of the classic `apt` tool with:

- Modern Rust implementation instead of C++
- Improved performance through parallelization
- Modern protocols (HTTP/2/3)
- Modern package format (.apx with zstd)
- Modern signatures (Ed25519 instead of GPG)

---

**Note**: This project is actively developed. Most core features are implemented. See [FUNCTIONS-LIST.md](docs/FUNCTIONS-LIST.md) for detailed status.
