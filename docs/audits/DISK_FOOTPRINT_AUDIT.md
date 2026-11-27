# AsterShell — Disk Footprint Audit

**Repository:** `medhansh5/AsterShell` (`C:\Users\medha\.gemini\antigravity\scratch\AsterShell`)  
**Branch:** `feat/m3-daily-driver-completion`  
**HEAD SHA:** `2c5e3812e935fa42bf79937e249123cdabc07bf8`  
**Drive C Free Space (Before):** `145.17 GB` (`329.94 GB` used)

---

## 1. Executive Summary

Recursive file-size measurement shows that the AsterShell repository consumes **19,881.52 MB (~19.42 GB)**, of which **19,879.35 MB (99.99%)** is inside `target/` (reproducible Rust build artifacts). Source code, documentation, and Git history (`.git`) total only **2.17 MB**. Shared Cargo registry caches (`735.23 MB`) and Rustup toolchains (`4,692.24 MB`) account for an additional **5,427.47 MB (~5.30 GB)** outside the repository.

---

## 2. Directory & Environment Size Breakdown

| Scope / Path | Recursive Size (MB) | Classification | Ownership |
|---|---:|---|---|
| **Repository Total** | **19,881.52** | Mixed | AsterShell |
| `target/` | 19,879.35 | Generated / Reproducible | AsterShell |
| `target/debug/` | 18,238.67 | Generated / Reproducible | AsterShell |
| `target/debug/build/` | 13,573.71 | Generated (`.pdb`, `.rlib`, `.exe`) | AsterShell |
| `target/debug/incremental/` | 4,165.54 | Generated (incremental cache) | AsterShell |
| `target/release/` | 1,640.67 | Generated / Reproducible | AsterShell |
| `target/release/build/` | 1,616.27 | Generated | AsterShell |
| `.git/` | 1.04 | Source-controlled metadata | AsterShell |
| `.git/objects/` | 0.98 | Git history objects | AsterShell |
| `docs/` | 0.47 | Source-controlled | AsterShell |
| `crates/` | 0.33 | Source-controlled | AsterShell |
| `apps/` | 0.17 | Source-controlled | AsterShell |
| **Cargo Registry (`~/.cargo/registry`)** | **735.23** | Shared Cache | Shared Rust |
| `~/.cargo/registry/src` | 603.52 | Shared unpacked crate sources | Shared Rust |
| `~/.cargo/registry/cache` | 89.14 | Shared `.crate` archives | Shared Rust |
| `~/.cargo/registry/index` | 42.57 | Shared crates.io index | Shared Rust |
| **Rustup (`~/.rustup`)** | **4,692.24** | Shared Toolchains (`stable`, `nightly`, `1.99.1`) | Shared System |
| **AsterShell AppData / Temp** | **0.00** | Runtime data | AsterShell |

---

## 3. Largest 30 Files in Repository

All 30 largest files reside inside `target/debug/` and are reproducible build outputs (`.pdb` debug symbol files for `taskbar`, `taskbar_foundation`, and `search_ui`, plus `libwindows-*.rlib`):

1. `target\debug\build\taskbar\1218f6d01ea9f8f6\out\taskbar.pdb` — **214.73 MB**
2. `target\debug\taskbar.pdb` — **214.73 MB**
3. `target\debug\build\taskbar\2bf63ba4ddf5b05b\out\taskbar.pdb` — **212.10 MB**
4. `target\debug\build\taskbar\73aa7556c8b9b242\out\taskbar.pdb` — **212.06 MB**
5. `target\debug\build\search-ui\937ca0fe87e2b900\out\search_ui.pdb` — **209.86 MB**
6. `target\debug\search_ui.pdb` — **209.86 MB**
7. `target\debug\build\taskbar\a3de03f217f0357f\out\taskbar.pdb` — **204.65 MB**
8. `target\debug\build\taskbar\39992a4cefed74fe\out\taskbar.pdb` — **204.09 MB**
9. `target\debug\build\taskbar\ffa30e8339bc3123\out\taskbar.pdb` — **201.75 MB**
10. `target\debug\build\taskbar\9d4c49624a48fa85\out\taskbar_foundation-9d4c49624a48fa85.pdb` — **194.07 MB**
11. `target\debug\build\taskbar\d2af49c8d677ef73\out\taskbar_foundation-d2af49c8d677ef73.pdb` — **192.39 MB**
12. `target\debug\build\taskbar\00515d27ff0b42f4\out\taskbar.pdb` — **187.10 MB**
13. `target\debug\build\taskbar\6c8db74aaa5fd3af\out\taskbar.pdb` — **185.61 MB**
14. `target\debug\build\taskbar\908c9e135486ec3f\out\taskbar_foundation-908c9e135486ec3f.pdb` — **177.54 MB**
15. `target\debug\build\taskbar\407ad287eb60a661\out\taskbar_foundation-407ad287eb60a661.pdb` — **176.50 MB**
16. `target\debug\build\taskbar\26f877a70f81e8ac\out\taskbar_foundation-26f877a70f81e8ac.pdb` — **174.94 MB**
17. `target\debug\build\taskbar\2c104df6dd236e59\out\taskbar_foundation-2c104df6dd236e59.pdb` — **174.48 MB**
18. `target\debug\build\taskbar\17f3662da1633524\out\taskbar_foundation-17f3662da1633524.pdb` — **173.64 MB**
19. `target\debug\build\taskbar\b83db287e12cbea2\out\taskbar-b83db287e12cbea2.pdb` — **144.12 MB**
20. `target\debug\build\search-ui\bc2e84cdcd41aab5\out\search_ui-bc2e84cdcd41aab5.pdb` — **132.75 MB**
21. `target\debug\build\search-ui\583a2644114ebd76\out\search_ui-583a2644114ebd76.pdb` — **132.09 MB**
22. `target\debug\build\search-ui\069fa94a0206b264\out\search_ui-069fa94a0206b264.pdb` — **131.10 MB**
23. `target\debug\build\taskbar\7271d2046680e04d\out\taskbar-7271d2046680e04d.pdb` — **129.82 MB**
24. `target\debug\build\taskbar\95426c6831c469d8\out\taskbar-95426c6831c469d8.pdb` — **128.33 MB**
25. `target\debug\build\taskbar\5cd4d22aecba9274\out\taskbar-5cd4d22aecba9274.pdb` — **119.68 MB**
26. `target\debug\build\taskbar\5355028c4681195b\out\taskbar-5355028c4681195b.pdb` — **119.66 MB**
27. `target\debug\build\windows\e8e201ad385661f2\out\libwindows-e8e201ad385661f2.rlib` — **118.40 MB**
28. `target\debug\build\taskbar\3b3c52a6cd2cf1c8\out\taskbar-3b3c52a6cd2cf1c8.pdb` — **117.08 MB**
29. `target\debug\build\taskbar\9195bd400e0327f9\out\taskbar-9195bd400e0327f9.pdb` — **114.50 MB**
30. `target\debug\build\taskbar\0a0bdbc03c757acd\out\taskbar-0a0bdbc03c757acd.pdb` — **114.01 MB**

---

## 4. Largest Directories in Repository

1. `target` — **19,879.35 MB**
2. `target\debug\build` — **13,573.71 MB**
3. `target\debug\incremental` — **4,165.54 MB**
4. `target\release\build` — **1,616.27 MB**
5. `.git` — **1.04 MB**
6. `docs` — **0.47 MB**
7. `crates` — **0.33 MB**
8. `apps` — **0.17 MB**

---

## 5. Recommended Cleanup & Optimization Actions

1. **Run `cargo clean` (100% reversible):** Reclaims **19,879.35 MB (~19.41 GB)** of accumulated multi-hash `.pdb` files, stale `target/debug/build` artifacts, `target/debug/incremental`, and `target/release`.
2. **Optimize Cargo profiles in root `Cargo.toml`:**
   - `[profile.dev] debug = "line-tables-only"` (or `debug = 1`): Reduces `.pdb` size for Slint + Winit + Win32 binaries by ~60–75% while preserving full file/line stack traces and backtraces.
   - `[profile.dev.package."*"] debug = 0`: Third-party crates (`slint`, `femtovg`, `resvg`, `icu_*`, `windows`) do not need full DWARF/PDB debug info during normal AsterShell development, drastically cutting `libwindows.rlib` and binary `.pdb` sizes.
   - `[profile.release] strip = "debuginfo"`: Strips bulky debuginfo from release binaries while keeping full `-O3` optimizations.
3. **Preserve shared caches & Git history:** Do not run `git gc --prune=now` (`.git` is only `1.04 MB`), do not delete `Cargo.lock`, and do not uninstall shared `~/.rustup` toolchains or `~/.cargo/registry`.
