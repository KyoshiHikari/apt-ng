# apt-ng — Technisches Design & PoC-Skeleton (Rust)

> Ziel: Eine moderne, deutlich schnellere Alternative zu `apt` / `apt-get` — implementiert in **Rust**. Dieses Dokument enthält Architektur, Paketformat, CLI-Spezifikation, Modulaufteilung, empfohlene Crates und ein kleines Proof-of-Concept (PoC) Projekt-Skeleton.

---

## 1. Kurzüberblick & Ziele

**Ziele:**

* Deutlich schnellere Paketoperationen (update, upgrade, install, remove).
* Multi-threaded, IO-parallelisiertes Herunterladen und Dekomprimieren.
* Modernes Paketformat (.apx) mit zstd und ed25519-Signaturen.
* SQLite-basierter Metadaten-Index für schnelle Suchen und atomare Updates.
* Effiziente, deterministische Abhängigkeitsauflösung (libsolv / eigener Solver).
* HTTP/2/3, Mirror-Selection und Delta-Updates.

Nicht-Ziele (Phase 1):

* vollständige Kompatibilität zu .deb-Installer-Skripten (preinst/postinst): Wir bieten Hooks, aber kein direktes Ausführen ungeprüfter Skripte in PoC.

---

## 2. High-Level Architektur

```
CLI -> Core Engine ->
  - Index (SQLite)
  - Downloader (HTTP/2/3, parallel)
  - Verifier (ed25519)
  - Decompressor (zstd streaming)
  - Solver (libsolv binding / rust-solver)
  - Installer (worker-pool, filesystem operations)
  - Repository manager (mirror tests, deltas)
```

Komponenten sind klar getrennt und kommunizieren über definierte Rust-APIs/Traits.

---

## 3. Paketformat: `.apx` (Design)

**Container-Layout (binary, stream-friendly):**

* Header (magic + version)
* metadata.json.zst (komprimiert, enthält Manifest: name, version, arch, deps, provides, files checksums, size, timestamp)
* content.tar.zst (streaming-tar komprimiert mit zstd)
* signature.ed25519 (Signatur über Header+metadata)

**Warum zstd?** Streaming-Dekompression, hohe Geschwindigkeit und guter Kompressionsgrad.

**Signatur:** Ed25519 (libsodium/ed25519-dalek). Kleines, schnelles Schema.

---

## 4. CLI-Spezifikation (erste Version)

```
apt-ng [GLOBAL OPTIONS]
Commands:
  update          Aktualisiert den lokalen Index (paralleles Herunterladen von Release-Dateien)
  search <term>   Sucht im lokalen SQLite-Index
  install <pkg>   Installiert ein oder mehrere Pakete (parallel download + solver)
  remove <pkg>    Entfernt Paket
  upgrade         Upgrades aller installierten Pakete
  show <pkg>      Zeigt Metadaten
  repo add <url>  Fügt ein Repo hinzu
  repo update     Testet Mirrors & aktualisiert prioritisation
  cache clean     Leert lokalen Cache

Options:
  -j, --jobs N    Anzahl paralleler Worker (Default: CPU * 2)
  --dry-run       Zeigt, was passieren würde
  --verbose
```

Die CLI ist minimal gehalten; alle Operationen sind deterministisch und idempotent.

---

## 5. Datenbank-Schema (SQLite)

Wichtige Tabellen (vereinfachte Darstellung):

* `packages (id INTEGER PRIMARY KEY, name TEXT, version TEXT, arch TEXT, provides JSON, depends JSON, size INTEGER, checksum TEXT, repo_id INTEGER, timestamp INTEGER)`
* `repos (id, url, priority, last_probe_ms, rtt_ms)`
* `installed (pkg_id, install_time, manifest JSON)`

Indexe auf `name`, `provides` und `timestamp`.

Atomic updates: `REPLACE INTO` in einer TRANSACTION; atomarer Swap der DB-Datei (VACUUM optional).

---

## 6. Downloader & Mirror Selection

* Verwendung von `reqwest` (HTTP/2) oder `hyper` für volle Kontrolle. Später Option für quic/h3 via `quinn`.
* Mirror-Selection: parallele Probes (ping + throughput sample), Ranking nach RTT×(1/throughput).
* Paralleles Chunked-Download für große Pakete (Range Requests) und paralleles Dekomprimieren.

---

## 7. Dependency Solver

Option A (empfohlen schnell): **libsolv** via FFI bindings (`libsolv-sys` / `bindgen`). libsolv ist sehr performant, battle-tested.

Option B: Minimaler eigenen SAT-basierten Solver (für bestimmten Use-Cases, höhere Implementations-Kosten).

API-Contract: Ein Trait `Solver` mit `solve(requested: &[PackageSpecifier]) -> Result<Solution>`.

---

## 8. Signatur & Verifikation

* Public keys in `trusted.gpg.d`-ähnlichem Verzeichnis, aber Ed25519.
* Signaturen mit `ed25519-dalek` prüfen — sehr schnell.

---

## 9. Installer

* Worker-Pool mit konfigurierbarer `-j`.
* Dekompression wird gestreamt in temporäre Pfade; Checksummen geprüft während des Streams.
* Atomic file moves: write to temp + `fs::rename`.
* Hooks: preinstall/postinstall Sandbox-API (Phase 1: nur controlled hooks).

---

## 10. Empfohlene Crates (Rust)

* `tokio` — async runtime
* `reqwest` oder `hyper` — http client
* `rusqlite` — sqlite
* `zstd` — compression
* `ed25519-dalek` — signatures
* `libsolv` binding crate (oder eigener)
* `rayon` — CPU-parallel tasks
* `anyhow` / `thiserror` — Fehlerhandling
* `clap` — CLI parsing

---

## 11. Sicherheitsüberlegungen

* Signaturen verpflichtend für Repo-Updates.
* Sandbox für Install-Hooks; in earliest PoC nur erlaubte, geprüfte Hooks.
* Least-privilege: Installer benötigt `CAP_SYS_ADMIN`? Ziel: Standard-Root-Permissions; dokumentieren.

---

## 12. PoC-Skeleton (Projektstruktur & minimale Dateien)

```
apt-ng-poc/
├─ Cargo.toml
├─ src/
│  ├─ main.rs
│  ├─ cli.rs
│  ├─ index.rs
│  ├─ downloader.rs
│  ├─ verifier.rs
│  └─ installer.rs
```

### Beispiel `Cargo.toml` (Auszug)

```
[package]
name = "apt_ng_poc"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4" }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["json","gzip","brotli","cookies","rustls-tls"] }
rusqlite = "0.29"
zstd = "0.12"
ed25519-dalek = "1"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rayon = "1.7"
```

### Minimaler `main.rs` (Skelett)

```rust
mod cli;
mod index;
mod downloader;
mod verifier;
mod installer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = cli::parse();
    // initialisiere DB / Config
    // match opts.command { ... }
    Ok(())
}
```

(Die kompletten Dateien sind als nächster Schritt implementierbar.)

---

## 13. Roadmap & Meilensteine

1. Phase 0 — Spezifikation & PoC-Skeleton (fertig)
2. Phase 1 — Index-Update + Downloader + SQLite-Index (PoC: `update`, `search`)
3. Phase 2 — Solver-Integration + Install-Pipeline (PoC: `install` einfache Pakete)
4. Phase 3 — Signatures + Security Hardening
5. Phase 4 — Performance-Tuning, delta-updates, HTTP/3

---

## 14. Tests & Benchmarks

* Unit-Tests für jede Komponente
* Integrationstest mit lokalen Repo (HTTP server)
* Benchmark: Vergleich `apt-get update && apt-get install` vs `apt-ng update/install` auf gleichen Packages

---

## 15. Nächste konkrete Schritte (ich erledige das für dich, wenn gewünscht)

* Vollständige Implementierung des PoC `update` + `search` (Rust) — ~Starter-PR
* Implementierung des parallelen Downloaders mit `reqwest` + zstd-Streaming
* Libsolv-Bindings oder einfacher Solver-Adapter

---
