# Component Implementation Status for **apt-ng**

This document lists all components and their implementation status.

## **1. CLI Functions**

* [x] `update` – Load repository metadata, validate, write to SQLite
* [x] `search` – Full-text and prefix search in local package index
* [x] `install` – Download, solver, verification, installation
* [x] `remove` – Uninstallation routine + consistency check
* [x] `upgrade` – Upgrades for all installed packages (fully implemented with dependency resolution)
* [x] `show` – Display package information from database
* [x] `repo add/remove` – Repository management
* [x] `cache clean` – Cache cleanup

---

## **2. Database / Index (SQLite)**

* [x] Schema finalized
* [x] Index update logic (Atomic swap DB)
* [x] Parser for Packages files (apt_parser.rs)
* [x] Parser for `metadata.json` of .apx packages (ApxPackage::open implemented)
* [x] Insert & update in SQLite index
* [x] Table for installed packages
* [x] DB migration system (migrate_repos_table, migrate_packages_table)

---

## **3. Downloader + Mirrors**

* [x] Parallel downloading of package lists
* [x] HTTP/2 client with throughput tests (reqwest with HTTP/2)
* [x] Mirror probing & ranking (probe_mirror implemented)
* [x] Range requests (chunk downloads) (download_file_chunked implemented)
* [x] Resume capability for interrupted downloads (resume_download implemented)
* [x] Checksum validation during download (download_file_with_checksum implemented)

---

## **4. Signatures & Security**

* [x] Ed25519-based repository signature verification (PackageVerifier implemented)
* [x] Keyring management for trusted keys (trusted_keys_dir, add_trusted_key)
* [x] Package signature verification (ApxPackage::verify_signature implemented and integrated in cmd_install)
* [x] Prevent unsigned/insecure repositories (implemented in cmd_update, verifies repository signatures)
* [x] Sandbox for install scripts (implemented with Bubblewrap integration)

---

## **5. Package Format **.apx****

* [x] Final format handling (Header, Magic, Version) - implemented for .deb
* [x] Zstd compression/decompression streaming (zstd crate available)
* [x] Parsing of metadata.json.zst (ApxPackage::open implemented)
* [x] Streaming extraction of content.tar.zst (ApxPackage::extract_to implemented)
* [x] Load and verify signature file (ApxPackage::verify_signature implemented)

---

## **6. Dependency Solver**

* [x] Binding to libsolv **or** custom Rust SAT solver (DependencySolver implemented)
* [x] Rules: depends, conflicts, provides, replaces (parsing implemented)
* [x] Version and architecture matching (select_best_version with version_matches implemented)
* [x] Creation of installation transaction (Solution struct with to_install/to_upgrade/to_remove)
* [x] Consistency check (prevent broken deps) (solve method with conflict detection)

---

## **7. Installer**

* [x] Worker pool for parallel decompression (worker_pool_size implemented)
* [x] Checksum verification during extraction (ApxPackage::verify_checksums implemented)
* [x] Atomic moves of files to target system (copy_directory_atomic with temp files + rename implemented)
* [x] Backup of existing files (optional) (add_backup in InstallationTransaction implemented)
* [x] Rollback mechanism for errors (InstallationTransaction::rollback implemented)
* [x] Simple pre/post hooks (run_hook skeleton available)
* [x] Sandbox support for hook execution (Bubblewrap integration)

---

## **8. Cache Management**

* [x] Storage location + cleanup rules (Cache struct, clean method)
* [x] Caching of already loaded packages (has_package, add_package)
* [x] Delta updates (DeltaCalculator, DeltaApplier, DeltaMetadata implemented)

---

## **9. Configuration System**

* [x] TOML-based main configuration (Config struct, toml crate)
* [x] Default paths (Linux: /etc/apt-ng, /var/lib/apt-ng, /var/cache/apt-ng)
* [x] Job settings (worker count etc.)
* [x] Sandbox configuration (enabled, network_allowed, memory_limit, cpu_limit)

---

## **10. Tests & Quality Assurance**

* [x] Unit tests for all modules (some tests available: cache, verifier, repo, index)
* [x] Integration tests with local test repository
* [x] Benchmarking tools against apt-get
* [x] Fuzzing for package format parsers (fuzz targets for Packages parser, .apx parser, dependency parser)
* [x] Security analysis (SecurityAudit, SecurityReport, security checks for signatures, sandbox, path traversal, input validation)

---

## **11. Repository Server**

* [x] Tool for creating .apx packages (apt-ng-build CLI, ApxBuilder, ApxSigner)
* [x] Repository index generator (RepositoryIndexGenerator, RepositorySigner, `apt-ng repo generate`)
* [x] Mini HTTP repository server for testing purposes (RepositoryServer, `apt-ng-server` CLI)
* [ ] CDN layout for production environments

---

## **12. Optimizations**

* [x] HTTP/3 QUIC download support (prepared, requires reqwest http3 feature to be stable)
* [x] Delta packages (DeltaCalculator and DeltaApplier framework implemented with xdelta3)
* [x] Transparent deduplication in cache (hard links based on SHA256 checksums)
* [x] Prefetching based on solver results (parallel downloads before installation)
* [x] Adaptive mirror selection with learning algorithm (RTT and throughput tracking)
* [x] Parallel SAT solver (experimental, using rayon for parallel dependency resolution)

---
