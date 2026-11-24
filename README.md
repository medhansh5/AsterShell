# AsterShell

[![CI](https://github.com/medhansh5/AsterShell/actions/workflows/ci.yml/badge.svg)](https://github.com/medhansh5/AsterShell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform: Windows 11](https://img.shields.io/badge/platform-Windows%2011-0078D6.svg)](https://microsoft.com/windows)

**An open-source, modern Windows desktop shell implemented in Rust, featuring a custom taskbar, Start menu, desktop, and Spotlight-style hybrid search.**

---

## 🌟 Vision & Overview

AsterShell is engineered to reimagine the Windows desktop experience. Built with native, memory-safe Rust and high-performance GPU-accelerated rendering, it offers a restrained, original, cohesive desktop environment that puts speed, aesthetics, and privacy first.

### Key Capabilities:
- **Spotlight-Style Hybrid Search:** Instant, sub-100ms global search (`Alt + Space`) unifying Windows Search indexed files, installed applications, fast math evaluation, and quick commands.
- **Modern Docked Taskbar:** Resilient, multi-monitor AppBar with live DWM window previews, pinned shortcuts, and zero OS bloat.
- **Fast Application Launcher:** Clean, keyboard-first Start experience reading the unified Windows application namespace.
- **Supervised Multi-Process Architecture:** Independent processes (`shell-host`, `taskbar`, `search-ui`, `start-menu`, `desktop`) orchestrated by a persistent supervisor with kernel-enforced crash recovery.
- **Dual Deployment Flexibility:** Operates seamlessly in **Companion Mode** alongside Windows Explorer, with an optional path toward **Full Shell Replacement**.

---

## 🏗️ Architecture

```
                                  +-----------------------+
                                  |      shell-host       | (Supervisor)
                                  +-----------+-----------+
                                              |
                   +--------------------------+--------------------------+
                   |  Windows Named Pipe IPC Bus (\\.\pipe\astershell-*) |
                   +--------------------------+--------------------------+
                                              |
             +--------------------+-----------+-----------+--------------------+
             |                    |                       |                    |
             v                    v                       v                    v
       +-----------+        +-----------+           +-----------+        +-----------+
       |  taskbar  |        | search-ui |           |start-menu |        |  desktop  |
       |  (AppBar) |        | (Alt+Space|           | (Launcher)|        | (Surface) |
       +-----------+        +-----------+           +-----------+        +-----------+
```

For complete technical specifications, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Reverse Engineering Investigation Plan](docs/reverse-engineering/INVESTIGATION_PLAN.md)
- [Explorer Feature Matrix](docs/reverse-engineering/FEATURE_MATRIX.md)
- [UI Technology ADR (ADR-0001)](docs/decisions/ADR-0001-ui-technology-selection.md)
- [Hybrid Search Engine Specification](docs/architecture/SEARCH_ENGINE_SPEC.md)
- [Process Supervision Specification](docs/architecture/PROCESS_SUPERVISION.md)
- [IPC Wire Protocol Specification](docs/architecture/IPC_PROTOCOL_SPEC.md)
- [Deployment & Fallback Strategy](docs/architecture/SHELL_DEPLOYMENT_STRATEGY.md)

---

## 🚀 Getting Started

### Prerequisites
- **Operating System:** Windows 10 (1809+) or Windows 11 (Recommended: Windows 11 Build 22000+)
- **Toolchain:** Rust stable (1.80+ or 1.100+)
- **Git:** 2.40+

### Building from Source

```bash
# Clone the repository
git clone https://github.com/medhansh5/AsterShell.git
cd AsterShell

# Verify workspace builds cleanly
cargo check

# Run the test suite across all 21 crates and apps
cargo test
```

---

## 🛡️ Safety & Non-Destructive Guarantee

AsterShell strictly adheres to a **Zero Shell Mutation Policy** during standard operation and development:
- **Companion Mode is Default:** AsterShell executes entirely in user mode alongside `explorer.exe`. It does not overwrite Windows system files or modify system-wide registry keys.
- **Zero Risk of Lockout:** If AsterShell is closed or terminates, Windows Explorer continues operating normally with zero disruption.

---

## 🗺️ Roadmap & Milestones

- **Phase 0:** Architecture, Reverse Engineering & Project Establishment ✅
- **Milestone 1:** IPC Foundation, Process Supervisor & Hybrid Search Core Prototype (In Progress)
- **Milestone 2:** Taskbar, Window Tracker & DWM Live Thumbnails
- **Milestone 3:** Start Menu & Unified Application Catalog
- **Milestone 4:** Desktop Canvas & Multi-Monitor Workspace Management
- **Milestone 5:** Optional Full Shell Replacement & Recovery Watchdog

See [ROADMAP.md](ROADMAP.md) for detailed milestone acceptance criteria.

---

## 📄 License & Community

- **License:** [MIT License](LICENSE)
- **Contributing:** See [CONTRIBUTING.md](CONTRIBUTING.md)
- **Security:** See [SECURITY.md](SECURITY.md)
- **Code of Conduct:** See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
