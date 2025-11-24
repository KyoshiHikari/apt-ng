---

# 🔧 Liste aller noch zu implementierenden Komponenten für **apt-ng**

## **1. CLI-Funktionen**

* [x] `update` – Repo-Metadaten laden, validieren, in SQLite schreiben
* [x] `search` – Volltext- und Prefixsuche im lokalen Paketindex
* [x] `install` – Download, Solver, Verifikation, Installation
* [x] `remove` – Deinstallationsroutine + Konsistenzprüfung
* [x] `upgrade` – Upgrades für alle installierten Pakete (vollständig implementiert mit Dependency-Resolution)
* [x] `show` – Paketinformationen aus der DB anzeigen
* [x] `repo add/remove` – Repo-Verwaltung
* [x] `cache clean` – Cache-Aufräumen

---

# 📚 Datenbank / Index (SQLite)

* [x] Schema finalisieren
* [x] Index-Update-Logik (Atomare Swap-DB)
* [x] Parser für Packages-Dateien (apt_parser.rs)
* [x] Parser für `metadata.json` der .apx-Pakete (ApxPackage::open implementiert)
* [x] Einfügen & Aktualisieren im SQLite-Index
* [x] Table für installierte Pakete
* [x] DB-Migrationssystem (migrate_repos_table, migrate_packages_table)

---

# 🌐 Downloader + Mirrors

* [x] Paralleles Herunterladen von Paketlisten
* [x] HTTP/2 Client mit Throughput-Tests (reqwest mit HTTP/2)
* [x] Mirror-Probing & Ranking (probe_mirror implementiert)
* [x] Range-Requests (Chunk-Downloads) (download_file_chunked implementiert)
* [x] Wiederaufnahme bei Unterbrechung (resume_download implementiert)
* [x] Checksummenvalidierung während Download (download_file_with_checksum implementiert)

---

# 🔐 Signaturen & Sicherheit

* [x] Ed25519-basierte Repo-Signaturprüfung (PackageVerifier implementiert)
* [x] Keyring-Management für trusted keys (trusted_keys_dir, add_trusted_key)
* [x] Überprüfen der Paket-Signaturen (ApxPackage::verify_signature implementiert und in cmd_install integriert)
* [x] Verhindern unsignierter/unsicherer Repos (in cmd_update implementiert, verifiziert Repository-Signaturen)
* [ ] Sandbox für Install-Skripte (später)

---

# 📦 Paketformat **.apx**

* [x] Finales Format-Handling (Header, Magic, Version) - für .deb implementiert
* [x] Zstd-Kompression/Decompression-Streaming (zstd crate vorhanden)
* [x] Parsing von metadata.json.zst (ApxPackage::open implementiert)
* [x] Streaming-Extraktion von content.tar.zst (ApxPackage::extract_to implementiert)
* [x] Signaturdatei laden und verifizieren (ApxPackage::verify_signature implementiert)

---

# 🧠 Dependency Solver

* [x] Binding zu libsolv **oder** eigener Rust SAT-Solver (DependencySolver implementiert)
* [x] Regeln: depends, conflicts, provides, replaces (parsing implementiert)
* [x] Version- und Architektur-Matching (select_best_version mit version_matches implementiert)
* [x] Erstellung einer Installations-Transaktion (Solution struct mit to_install/to_upgrade/to_remove)
* [x] Konsistenzprüfung (broken deps verhindern) (solve method mit Konflikt-Erkennung)

---

# 🛠 Installer

* [x] Worker-Pool zur parallelen Dekompression (worker_pool_size implementiert)
* [x] Prüfen von Checksummen beim Entpacken (ApxPackage::verify_checksums implementiert)
* [x] Atomic Moves von Dateien ins Zielsystem (copy_directory_atomic mit temp files + rename implementiert)
* [x] Backup bestehender Dateien (optional) (add_backup in InstallationTransaction implementiert)
* [x] Rollback-Mechanismus bei Fehlern (InstallationTransaction::rollback implementiert)
* [x] Einfache pre/post Hooks (run_hook Skelett vorhanden)

---

# 🗃 Cache-Management

* [x] Speicherort + Cleanup-Regeln (Cache struct, clean method)
* [x] Caching von bereits geladenen Paketen (has_package, add_package)
* [ ] Delta-Updates (optional später)

---

# ⚙ Konfigurationssystem

* [x] TOML-basierte Hauptkonfiguration (Config struct, toml crate)
* [x] Default-Pfade (Linux: /etc/apt-ng, /var/lib/apt-ng, /var/cache/apt-ng)
* [x] Job-Einstellungen (Worker-Anzahl etc.)

---

# 🧪 Tests & Qualitätssicherung

* [x] Unit-Tests aller Module (einige Tests vorhanden: cache, verifier, repo, index)
* [ ] Integrationstests mit lokalem Test-Repo
* [ ] Benchmarking-Tools gegen apt-get
* [ ] Fuzzing für Paketformat-Parser
* [ ] Sicherheitsanalyse (Signaturen & Hook-Sandbox)

---

# 🖥 Repo-Server (optional, für später)

* [ ] Werkzeug zum Erstellen von .apx-Paketen
* [ ] Repository-Index-Generator
* [ ] Mini-HTTP-Repo-Server für Testzwecke
* [ ] CDN-Layout für Produktivumgebungen

---

# 🚀 Optimierungen (nach dem MVP)

* [ ] HTTP/3 QUIC-Download-Unterstützung
* [ ] Delta-Pakete (nur geänderte Daten laden)
* [ ] Transparente Deduplizierung im Cache
* [ ] Prefetching basierend auf Solver-Ergebnissen
* [ ] Adaptive Mirror-Selection mit Lern-Algorithmus
* [ ] Paralleler SAT-Solver (experimentell)

---
