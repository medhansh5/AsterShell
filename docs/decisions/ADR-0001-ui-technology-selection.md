# ADR-0001: UI Technology Selection for AsterShell

- **Status:** Proposed / Accepted for Prototyping
- **Date:** 2026-11-24
- **Deciders:** Principal Systems Architect, Senior Rust Engineer
- **Consulted:** Reverse Engineering Findings (EV-005, EV-006)

---

## 1. Context and Problem Statement

AsterShell requires a modern, responsive, high-performance UI framework for its graphical components (`taskbar`, `search-ui`, `start-menu`, `desktop`, `settings`). The user experience mandates:
- Sub-50ms cold window rendering and immediate interactive response.
- Flawless Per-Monitor V2 DPI scaling across displays with mixed refresh rates (60Hz to 240Hz).
- Native Windows 11 visual backdrops (Mica, Acrylic, blur) rendered seamlessly over desktop surfaces.
- First-class accessibility via Windows UI Automation (UIA).
- Clean architectural separation between UI presentation and low-level Win32 platform integration.

We evaluated four UI technology candidates:
1. **Slint** (Declarative, reactive Rust UI toolkit)
2. **WinUI 3 via Windows App SDK / WinRT** (Microsoft's modern shell framework)
3. **Native Win32 with DirectComposition / Direct2D** (Lowest overhead, manual composition)
4. **egui** (Immediate-mode Rust GUI)

---

## 2. Evaluation Matrix

| Criterion (Weight) | Slint | WinUI 3 | Native Win32 + DirectComposition | egui |
| :--- | :---: | :---: | :---: | :---: |
| **Native HWND & AppBar Docking (20%)** | 9/10 | 5/10 | 10/10 | 6/10 |
| **Rendering Perf & Frame Rates (20%)** | 9/10 | 6/10 | 10/10 | 7/10 |
| **Memory Footprint & Startup Time (15%)** | 9/10 (15-25 MB) | 4/10 (90-140 MB) | 10/10 (5-10 MB) | 9/10 (12-20 MB) |
| **Backdrop Effects (Mica/Acrylic) (10%)** | 9/10 | 10/10 | 10/10 | 5/10 |
| **Animation & Declarative Layout (10%)** | 10/10 | 8/10 | 4/10 | 5/10 |
| **Accessibility (UI Automation) (10%)** | 9/10 (AccessKit) | 10/10 (Native) | 3/10 (Manual COM) | 5/10 |
| **Rust Toolchain & Build Health (15%)** | 10/10 | 3/10 | 8/10 | 10/10 |
| **Weighted Score (100%)** | **9.1 / 10** | **6.1 / 10** | **7.8 / 10** | **7.0 / 10** |

---

## 3. In-Depth Candidate Analysis

### Candidate 1: Slint (Preferred Candidate)
- **Strengths:** Compiled DSL (`.slint`) that produces type-safe Rust code with zero runtime reflection overhead. Native integration with `winit` allowing direct access to underlying `HWND` handles for `SHAppBarMessage` docking, DWM window attributes (`DWMWA_SYSTEMBACKDROP_TYPE`), and multi-monitor repositioning. Native AccessKit integration satisfies Windows accessibility out-of-the-box. Fast cold start (< 30 ms) and low memory profile.
- **Weaknesses:** Relatively new ecosystem compared to Qt; requires custom theme tokens to emulate custom modern aesthetics.
- **Suitability:** High. Ideal balance of developer ergonomics, rendering performance, and native Windows integration.

### Candidate 2: WinUI 3 / Windows App SDK
- **Strengths:** Exact match for Windows 11 native aesthetics; native XAML controls.
- **Weaknesses:** Extremely hostile to idiomatic Rust development. Requires extensive C++/WinRT interop bridges, introduces a heavy runtime dependency (`Microsoft.WindowsAppRuntime` MSIX packages), consumes 100+ MB per process, and suffers from noticeable initialization lag (200-400 ms cold start).
- **Suitability:** Rejected for core shell processes. Too heavy and brittle for a high-performance shell companion.

### Candidate 3: Native Win32 + DirectComposition
- **Strengths:** Absolute minimum memory footprint (5-10 MB), zero abstraction overhead, instant startup, total control over DWM composition trees and swapchains.
- **Weaknesses:** Developing complex modern UI components (search fuzzy list, auto-scrolling pinned apps, responsive settings) requires writing hundreds of lines of boilerplate layout math, text shaping with DirectWrite, and manual hit-testing. Enormous engineering maintenance burden.
- **Suitability:** Maintained as reference benchmark and reserved for low-level window-tracking preview overlays.

### Candidate 4: egui
- **Strengths:** Fast to write, immediate-mode convenience, zero build dependencies.
- **Weaknesses:** Immediate-mode architecture continuously re-evaluates UI trees, leading to higher CPU wakeups on battery. Subpixel font rendering and fractional HiDPI scaling are visually inferior to retained-mode vector engines. Lacks native DWM transparency blending and robust accessibility trees.
- **Suitability:** Rejected for primary desktop shell UI; acceptable only for internal developer diagnostics panels.

---

## 4. Decision

We decide to adopt **Slint** as the primary UI technology for AsterShell's user-facing components (`taskbar`, `search-ui`, `start-menu`, `settings`), with the following strict architectural constraints:

1. **Isolation of UI from Platform:** Slint components will contain **zero** direct Win32 or COM calls. All platform operations (window tracking, AppBar docking, hotkey dispatching, file launching) must reside in `platform-windows` and `shell-api` and communicate with the Slint layer via pure Rust event channels or callbacks.
2. **Prototype Validation Milestone:** Prior to committing to full shell UI implementation in Phase 2, a standalone prototype of `search-ui` and `taskbar` will be benchmarked to verify:
   - HWND docking stability under display resolution changes.
   - P95 query-to-render latency under 50 ms.
   - Visual fidelity of Mica/Acrylic backdrop composition.
3. **DirectComposition Fallback:** If Slint encounters an insurmountable blocker regarding multi-monitor DWM live thumbnail blending, `ui-components` will expose a native DirectComposition surface bridge.
