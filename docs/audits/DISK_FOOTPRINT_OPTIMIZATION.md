# AsterShell — Disk Footprint Optimization Report

**Repository:** `medhansh5/AsterShell` (`C:\Users\medha\.gemini\antigravity\scratch\AsterShell`)  
**Branch:** `feat/m3-daily-driver-completion`  
**Audit Baseline:** [DISK_FOOTPRINT_AUDIT.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/audits/DISK_FOOTPRINT_AUDIT.md)  
**Status:** `OPTIMIZATION COMPLETE — FOOTPRINT REDUCED`

---

## 1. Before-and-After Measurements

All "After" measurements were captured **after** running `cargo clean` AND executing a full clean-state rebuild (`cargo fmt`, `cargo check --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked -j 2`), representing realistic active-development disk usage rather than an empty `target/`.

| Measurement | Before (MB) | After Clean + Full Test Rebuild (MB) | Net Saved (MB) | Notes |
|---|---:|---:|---:|---|
| **Repository total** | **19,881.52** | **2,511.60** | **17,369.92 (~16.96 GB)** | `19.4 GiB` reclaimed on `cargo clean`; hot rebuilt workspace is `2.45 GB`. |
| `.git/` | 1.04 | 1.04 | 0.00 | 100% preserved; no `git gc` or history rewrite. |
| `target/` | 19,879.35 | 2,509.42 | 17,369.93 | Multi-hash stale `.pdb`s removed + smaller `line-tables-only` PDBs. |
| `target/debug/incremental/` | 4,165.54 | 429.56 | 3,735.98 | 89.7% smaller incremental cache. |
| Largest single `.pdb` (`taskbar.pdb`) | 214.73 | 33.75 | 180.98 | 84.3% reduction per binary PDB. |
| **Shared Cargo registry (`~/.cargo/registry`)** | 735.23 | 735.23 | 0.00 | Shared Rust cache (preserved). |
| **Shared Rustup (`~/.rustup`)** | 4,692.24 | 4,692.24 | 0.00 | Shared system toolchains (`stable`, `nightly`, `1.99.1`) (preserved). |
| **AsterShell AppData / Temp** | 0.00 | 0.00 | 0.00 | — |
| **Total AsterShell-related dev footprint** | **25,308.99** | **7,939.07** | **17,369.92** | Drive C free space grew from `145.17 GB` to `161.16 GB` (`+15.99 GB`). |

---

## 2. Cleanup & Optimization Actions Performed

1. **Safe Artifact Cleanup (`cargo clean`):**
   - Removed `39,525` generated files (`19.4 GiB`) from `target/`, eliminating dozens of stale 114–214 MB `.pdb` files and old `target/release` artifacts.
2. **Long-Term Cargo Profile Optimization (`Cargo.toml`):**
   - `[profile.dev] debug = "line-tables-only"`: Preserves full file/line stack traces and panic backtraces while cutting `.pdb` sizes by ~84%. Incremental compilation remains enabled for fast edit-compile cycles.
   - `[profile.dev.package."*"] debug = 0`: Avoids generating hundreds of MB of debuginfo for third-party dependencies (`slint`, `femtovg`, `resvg`, `icu_*`, `windows`).
   - `[profile.release] strip = "debuginfo"`: Strips bulky debuginfo from release binaries without altering `-O3` optimizations.
3. **Repository Hygiene (`.gitignore`):**
   - Added `/target-*/`, `*.dmp`, `*.tmp`, `*.bak`, `.env`, `.env.*`.
   - Verified via `git ls-files` that the largest tracked file in Git is `Cargo.lock` (`151.4 KB`); zero generated binaries are tracked.

---

## 3. Verification Results (Clean State)

- `cargo fmt --all -- --check`: **PASS**
- `cargo check --workspace --all-targets --locked`: **PASS**
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: **PASS**
- `cargo test --workspace --locked -j 2`: **PASS** (0 failures, 0 warnings)
- `git diff --stat Cargo.lock`: **0 changes** (lockfile untouched)

---

## 4. Recommended Periodic Maintenance

```powershell
# Reclaim stale multi-hash debug artifacts when target/ grows over time:
cargo clean

# Or sweep only release artifacts after benchmarking:
cargo clean --release
```
