# AsterShell Development & Governance Policies

## 1. License Notice
Copyright (c) 2026 AsterShell Project Contributors.
This software is distributed under the terms of the MIT License.

## 2. Code of Conduct & Community Standards
AsterShell maintains an open, welcoming, and harassment-free environment for all participants.
- Treat every contributor with professionalism and mutual respect.
- Provide objective, constructive feedback during architectural discussions and code reviews.
- Focus on technical merit, empirical data, code clarity, and system security.

## 3. Contributing Guidelines
- All pull requests must compile cleanly on the latest stable Rust toolchain on Windows.
- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test` prior to submitting code.
- New architecture decisions must be accompanied by an ADR in `docs/decisions/`.
- Unsafe Rust code must be strictly isolated to `platform-windows` or `crates/` FFI boundaries and documented with `// SAFETY:` invariant comments.

## 4. Security & Vulnerability Reporting
If you discover a potential vulnerability in AsterShell (particularly around IPC pipe authorization, process supervision, or arbitrary code execution), please do not report it in public GitHub issues. Instead, email security disclosures to `security@astershell.dev` or use GitHub Private Vulnerability Reporting.
