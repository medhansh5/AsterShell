# AsterShell — Milestone 3: UI & Interaction Specification (`AsterTaskbar`)

**Milestone:** M3 — Custom Taskbar & System Tray Integration  
**Component:** `AsterTaskbar` UI (`apps/taskbar` & Slint Design System)  
**Status:** DRAFT SPECIFICATION (APPROVED FOR M3 PREFLIGHT)  
**Target Platform:** Windows 11 (Build 22000+)  
**Baseline Commit:** `31a1a09fa5ab993c873133b891b28f7c3641dcdc`

---

## 1. Visual Identity & Design Philosophy

AsterShell's visual identity embodies **Modern Precision**: a design language that feels completely native to Windows 11 while remaining distinctly original, minimalist, and lightweight.

AsterTaskbar avoids reproducing proprietary Windows assets or creating a pixel-perfect clone of Explorer's taskbar. Instead, it introduces an original shell companion aesthetic:
- **Spatial Restraint:** Generous negative space, crisp micro-paddings, and zero cluttered borders.
- **Glass & Depth:** True Windows 11 DWM Mica / Acrylic system backdrops with graceful fallback to translucent neutral palettes.
- **Micro-Interactions:** Fluid 120ms hover transitions, subtle active-window indicator pills, and clear state hierarchy.
- **Information Density:** High clarity without visual noise; status indicators communicate through clean vector glyphs and concise typography.

---

## 2. Layout Concepts & Recommendation

To determine the optimal taskbar structure for companion deployment, two distinct layout concepts were engineered and evaluated:

```
+-------------------------------------------------------------------------------------------------------+
|  CONCEPT 1: FLOATING COMPANION ISLAND (RECOMMENDED)                                                   |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|    +---------------------------------------------------------------------------------------------+    |
|    | [ * ] | [ â–£ ] [ â—† ] [ â—‹ ] [ â–³ ] | [ Terminal: zsh ] [ Code ] | [ 100% ] [ â˜° ] [ 4:20 PM ] |    |
|    +---------------------------------------------------------------------------------------------+    |
|       ^                   ^                             ^                           ^                 |
|    Launcher        Pinned Apps Grid              Running Tasks             System Indicators & Clock  |
|                                                                                                       |
|  * Segmented, floating translucent pill docked at screen top or offset from edges.                   |
|  * Rounded corners (DWMWCP_ROUND), elevated above desktop without contracting work area.             |
|  * Coexists harmoniously with any existing Windows taskbar setup.                                     |
|                                                                                                       |
+-------------------------------------------------------------------------------------------------------+
|  CONCEPT 2: EDGE-TO-EDGE SYSTEM APPBAR                                                                |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|  +-------------------------------------------------------------------------------------------------+  |
|  | [ * ]  [ â–£ ] [ â—† ] [ â—‹ ]  |  [ Terminal: zsh ] [ VS Code ]                 | [ â˜° ] [ 4:20 PM ] |  |
|  +-------------------------------------------------------------------------------------------------+  |
|                                                                                                       |
|  * Full-width edge bar docked across entire monitor width (top or bottom).                            |
|  * Sharp corners (DWMWCP_DONOTROUND), registered as formal Win32 AppBar (SHAppBarMessage).            |
|  * Adjusts desktop work area (SPI_SETWORKAREA) so maximized windows snap cleanly beneath it.          |
|                                                                                                       |
+-------------------------------------------------------------------------------------------------------+
```

### 2.1 Concept Comparison & Trade-Off Analysis

| Criteria | Concept 1: Floating Companion Island | Concept 2: Edge-to-Edge System AppBar |
| :--- | :--- | :--- |
| **Explorer Coexistence** | **Superior:** Floats effortlessly without fighting Explorer work-area calculations. | **Moderate:** Requires AppBar registration (`ABM_SETPOS`) which modifies global work area. |
| **Visual Distinction** | **High:** Instant original identity; distinct from standard OS taskbar. | **Low:** Can feel like a second traditional taskbar competing for space. |
| **Maximized Windows** | Floats over top region (or auto-drops on fullscreen). | Maximized windows dock cleanly against edge without occlusion. |
| **Multi-Monitor Scaling** | Easily centered on each monitor regardless of display size differences. | Stretches to full monitor width on each display. |
| **Resource Footprint** | Extremely low (compact rendering bounds). | Low (requires full-screen-width repaint surface). |

### 2.2 Product Direction & Operating Mode Classification (P2-01)
To eliminate any contradiction between companion overlay behavior and formal shell docking, AsterShell establishes two separate product classifications:

1. **Active Target: M3A Floating Companion Dock (Concept 1):**
   - **Status:** **Active & Exclusively Implemented in Milestone 3 (M3A–M3F).**
   - **Behavior:** Operates purely as a floating, non-activating companion window (`WS_POPUP`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, `WS_EX_TOPMOST`).
   - **No AppBar Calls:** Strictly does **NOT** invoke `SHAppBarMessage` (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`).
   - **No Work-Area Modification:** Does **NOT** alter the Windows desktop work area (`SPI_SETWORKAREA`).
   - **Explorer Coexistence:** The Windows 11 Explorer taskbar remains 100% untouched and active at the bottom of the screen.
   - **Fitts's Law & Tab Strip Protection:** Placed flush against the monitor top boundary (`top = 0`) to preserve upward flick target depth, while auto-demoting to `HWND_BOTTOM` on borderless/exclusive fullscreen.
2. **Future Target: Edge-Anchored System AppBar (Concept 2):**
   - **Status:** **Deferred post-M3 / Milestone 6 (Full Shell Replacement) Optional Feature.**
   - **Behavior:** Registered Win32 AppBar contracting desktop work area via `ABM_SETPOS`.
   - **Explicit Scope Boundary:** **STRICTLY PROHIBITED IN M3A AND M3.** Will not be implemented until full shell replacement is authorized.

---

## 3. Layout Hierarchy & Geometry

### 3.1 Dimensions & Metric Tokens (at 100% DPI / 96 DPI Baseline)

| Metric Token | Logical Dimension | Scaled at 150% DPI | Scaled at 200% DPI | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| `taskbar-height` | `48px` | `72px` | `96px` | Total height of the taskbar bar/island. |
| `island-padding-x` | `12px` | `18px` | `24px` | Left and right internal margin of the island. |
| `island-padding-y` | `6px` | `9px` | `12px` | Top and bottom internal padding. |
| `island-corner-radius`| `16px` | `24px` | `32px` | DWM window and container corner radius. |
| `button-size` | `36px x 36px` | `54px x 54px` | `72px x 72px` | Standard touch/click target for app buttons. |
| `icon-size` | `22px x 22px` | `33px x 33px` | `44px x 44px` | Central application icon rendering box. |
| `item-spacing` | `4px` | `6px` | `8px` | Gap between adjacent taskbar items. |
| `section-separator` | `1px x 20px` | `1.5px x 30px` | `2px x 40px` | Subtle divider between pinned and running tasks. |

### 3.2 Structural Zones (Left-to-Right in LTR Layout)
1. **Zone 1: Shell Anchor (`LauncherButton`)**
   - AsterShell branding glyph (`*`).
   - Clicking toggles the Spotlight Search overlay (`apps/search-ui`).
2. **Zone 2: Pinned Applications (`PinnedAppSection`)**
   - User-curated favorite applications.
   - If an application is also running, an active indicator dot appears beneath its icon.
3. **Zone 3: Divider (`Separator`)**
   - Subtle vertical hairline (`1px`, `#2e2e38`).
4. **Zone 4: Running Application Tasks (`RunningTaskSection`)**
   - Active top-level application windows.
   - Distinct visual state for active foreground window versus background running windows.
5. **Zone 5: System Indicators & Clock (`StatusSection`)**
   - Battery level badge (on laptops).
   - Network connectivity glyph (Ethernet / Wi-Fi).
   - Volume status glyph.
   - Digital Clock (HH:MM) with date tooltip.

---

## 4. Visual States & Design Tokens

### 4.1 Color Palette & Theme Tokens

```
+---------------------------------------------------------------------------------------+
| TOKEN NAME                 | VALUE (DARK THEME)       | VALUE (LIGHT THEME)           |
+----------------------------+--------------------------+-------------------------------+
| `surface-backdrop`         | rgba(20, 20, 24, 0.88)   | rgba(245, 245, 248, 0.88)     |
| `surface-border`           | rgba(255, 255, 255, 0.08)| rgba(0, 0, 0, 0.08)           |
| `item-hover-bg`            | rgba(255, 255, 255, 0.08)| rgba(0, 0, 0, 0.06)           |
| `item-active-bg`           | rgba(255, 255, 255, 0.14)| rgba(0, 0, 0, 0.10)           |
| `indicator-active`         | #3b82f6 (Accent Blue)    | #2563eb (Deep Blue)           |
| `indicator-running`        | #9ca3af (Neutral Silver) | #6b7280 (Neutral Slate)        |
| `text-primary`             | #f4f4f6                  | #18181b                       |
| `text-secondary`           | #a1a1aa                  | #71717a                       |
+---------------------------------------------------------------------------------------+
```

### 4.2 Interactive States
- **Idle (Normal):** Icon centered at 100% opacity, transparent item background.
- **Hover:**
  - Background fades to `item-hover-bg` via smooth 120ms ease-out transition.
  - Subtle 1px elevation or subtle glow.
  - Tooltip card displays window title after 300ms hover delay.
- **Active (Foreground Window):**
  - Item background set to `item-active-bg`.
  - Solid accent indicator pill (`16px x 3px`, `#3b82f6`) anchored to bottom of button.
- **Running (Background Window):**
  - Small neutral dot (`4px x 4px`, `#9ca3af`) anchored to bottom of button.
- **Pressed (Click-Down):**
  - Scale down to 96% (`transform: scale(0.96)`) for tactile click feedback.

---

## 5. Interaction Model & Keyboard Accessibility

### 5.1 Mouse Interactions
- **Single Left-Click:**
  - *On inactive task:* Restores if minimized, activates window to foreground (`SetForegroundWindow`).
  - *On active foreground task:* Minimizes the window (`ShowWindow(SW_MINIMIZE)`).
  - *On pinned app (not running):* Launches application via `shell-api`.
  - *On Launcher button:* Toggles Spotlight search launcher.
- **Middle-Click (Wheel Click):**
  - Launches a new instance of the clicked application (standard Windows power-user behavior).
- **Right-Click:**
  - *On application task:* Opens AsterTaskbar Task Context Menu (Restore, Minimize, Maximize, Close Window, Unpin).
  - *On empty taskbar space:* Opens Taskbar Settings & Management Context Menu.

### 5.2 Keyboard Navigation & Focus Model
AsterTaskbar provides full keyboard accessibility:
1. **Global Shortcut to Focus Taskbar:** `Win + Alt + T` focuses the AsterTaskbar navigation ring.
2. **Arrow Key Traversal:**
   - `LeftArrow` / `RightArrow`: Cycles through launcher, pinned apps, running tasks, and status indicators.
   - Visual focus ring: 2px high-contrast accent ring (`#3b82f6`).
3. **Activation:**
   - `Enter` / `Space`: Activates the selected item.
4. **Context Menu:**
   - `Shift + F10` / `AppMenuKey`: Opens context menu for currently focused task.
5. **Dismiss:**
   - `Escape`: Releases taskbar focus and returns foreground focus to previously active window.

---

## 6. Accessibility, Motion & Contrast Modes

### 6.1 Reduced Motion Mode
- When Windows setting `SPI_GETCLIENTAREAANIMATION` or `UI Settings.AdvancedEffectsEnabled` is disabled:
  - All hover and state transitions switch from 120ms animations to instant 0ms state switches.
  - Scale animations during click-down are disabled.

### 6.2 High Contrast Mode
- When Windows High Contrast Mode is active (`SystemParametersInfoW(SPI_GETHIGHCONTRAST)`):
  - Translucent backdrops are replaced with solid 100% opaque system window background (`COLOR_WINDOW`).
  - Borders are forced to 2px solid system text color (`COLOR_WINDOWTEXT`).
  - Active indicator pills render with system highlight color (`COLOR_HIGHLIGHT`).

---

## 7. Slint Component Architecture

AsterTaskbar UI is structured into modular Slint components:

```slint
// apps/taskbar/ui/taskbar.slint

export component TaskbarWindow inherits Window {
    title: "AsterShell Taskbar";
    no-frame: true;
    background: transparent;

    in-out property <[TaskItemData]> pinned_tasks: [];
    in-out property <[TaskItemData]> running_tasks: [];
    in-out property <string> clock_text: "12:00";
    in-out property <float> volume_level: 0.75;
    in-out property <int> battery_percent: 100;
    in-out property <bool> is_on_ac: true;

    callback task_activated(int);
    callback task_middle_clicked(int);
    callback task_context_requested(int);
    callback launcher_clicked();

    HorizontalLayout {
        alignment: center;
        
        // Main Island Container
        Rectangle {
            background: #141418;
            border-radius: 16px;
            border-width: 1px;
            border-color: #2e2e38;

            HorizontalLayout {
                padding: 6px;
                spacing: 4px;

                // 1. Launcher Button
                LauncherButton { clicked => { root.launcher_clicked(); } }

                // 2. Pinned Apps
                for task in root.pinned_tasks: TaskButton { ... }

                // 3. Separator
                Rectangle { width: 1px; background: #2e2e38; }

                // 4. Running Tasks
                for task in root.running_tasks: TaskButton { ... }

                // 5. Separator
                Rectangle { width: 1px; background: #2e2e38; }

                // 6. System Status & Clock
                StatusGroup { ... }
            }
        }
    }
}
```

---

## 8. References
1. [Slint UI Framework Documentation: Window Handling](https://slint.dev/docs/rust/slint/)
2. [Windows Design Guidelines: Layout & Micro-Interactions](https://learn.microsoft.com/en-us/windows/apps/design/)
3. [Microsoft Learn: Supporting High Contrast Themes](https://learn.microsoft.com/en-us/windows/win32/w8cookbook/high-contrast-themes)
