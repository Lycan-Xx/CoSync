# Cosync — Development Specification (v2)

## How to Use This Document

Each milestone is self-contained: **Goal → Prerequisites → Iterations
→ Acceptance Criteria → Context Handoff**. Hand one milestone to a
developer or AI agent at a time. Never start the next milestone until
the current one's Acceptance Criteria pass.

**The iteration model:** Every feature follows three passes. Don't
skip ahead. Don't polish before it works. Don't make it pretty before
it's correct.

```
Iteration 1 — Functional:   it works, wiring is real, UI is minimal
Iteration 2 — Designed:     matches the design system, looks right
Iteration 3 — Polished:     keyboard nav, animations, edge cases, a11y
```

Each milestone specifies which iterations are in scope. Iterations
not listed are deferred.

**What was decided along the way:**
- Mobile: React Native (Expo bare workflow), not Flutter
- macOS: explicitly out of scope (LinkMyMac already owns it)
- No cloud relay of any kind, ever
- Virtual webcam/mic/SMS/calls: deferred to v2.0 (Milestone 10)
- Milestone 0–2 complete and pushed (Rust core, discovery, pairing,
  paired-device persistence)
- Milestone 3 Rust backend complete; React frontend not yet built

---

## Design System Reference

Everything built from here forward must conform to this system.
Read this before writing a single line of UI code.

### Desktop Design Language

**Window treatment**
- Native OS window decorations (standard title bar with min/max/close)
- Native desktop proportions — no full-screen, no large panels
- Subtle 8px corners where custom rounding applies
- Dark graphite background: approx `#1a1a1f` surface, `#222228`
  slightly lighter panels
- Low-contrast borders: `1px solid rgba(255,255,255,0.08)`
- No decorative shadows, no gradients

**Typography**
- System sans-serif stack: `Segoe UI, Inter, Ubuntu Sans, sans-serif`
- Hierarchy from size and weight only, never color
- 12–14px inline, 20px hero labels only
- 11–13px base text, 10px eyebrows (section labels, metadata)
- Monospace only for device IDs, fingerprints, shortcuts, code snippets

**Color & status**
- Primary surface: `#1a1a1f`
- Panel: `#222228`
- Text primary: `rgba(255,255,255,0.87)`
- Text secondary: `rgba(255,255,255,0.45)`
- Accent blue: `#3b82f6` — used for toggles, selection, active state,
  progress fills only
- Green `#22c55e` dot: connected state only, never a badge
- Amber `#f59e0b` dot: reconnecting state only
- Gray dot: offline state
- Red `#ef4444`: destructive actions only (Forget device, etc.)
- Never use color to convey hierarchy — only status

**Icons**
- Fluent UI / Windows 11 line style
- Stroke 1.4, rounded joins, no fills except status indicators
- 16px default, 12–14px inline, 20px for section heroes
- Icons support text labels; they never replace them

**Interaction**
- Keyboard-first: every launcher and list accepts arrow keys + Enter
- No home screen; tray → feature directly
- Show only relevant state; hide everything else
- Native controls only: switches, dropdowns, context menus
- No animated loading spinners unless a network round-trip is in
  progress and takes >300ms

**What to avoid (hard rules)**
- No large dashboard or home page
- No analytics or "recent activity" cards
- No decorative gradients or oversized CTAs
- No SaaS-style sidebar navigation outside the Settings window
- No mobile-stretched layouts or status badges
- No web-app design patterns: rounded hero cards, large thumbnails,
  app-store-style feature rows

### Mobile Design Language

**Overall model**
- Two pages only: Home (operational) and Control (trust/permissions)
- Bottom nav: `Home` | `Control`
- Most interactions happen in Android system surfaces (share sheet,
  notification shade, quick settings) — the app itself is minimal

**Visual language**
- Background: `#0f1117` (slightly deeper than desktop)
- Surface: `#1a1c23`
- Card/section: `#21242d`
- Same accent blue, same status dots as desktop
- System sans-serif, same size scale

**Home page structure (confirmed layout)**
```
[ App bar: Cosync logo + overflow ⋮ ]

[ Connection card — large, ~45–50% initial viewport ]
  DESKTOP-ATELIER
  ● Connected
  Wi-Fi · Local network

[ Clipboard section ]
  [ Search clipboard ]
  [ list of history items ]

[ Bottom nav: Home | Control ]
```

NOTE: The three trust toggle cards (Clipboard / Files / Notifications)
that appeared in early mockups are NOT on the Home page. They live
exclusively on the Control page. The connection card on Home should be
large and clear. Do not add them back.

**Home scrolled state**
When the user scrolls clipboard history, the connection card collapses
to a single compact line:
```
DESKTOP-ATELIER  ●  Connected · Wi-Fi · Local network
```
The clipboard list then has ~80% of the viewport.

**Control page structure**
```
[ Connection card — compact ]

ALLOW THIS PC TO ACCESS
  Clipboard         [toggle]
  Files             [toggle]
  Notifications     [toggle]

SYSTEM STATUS
  Background access       Running normally  >
  Battery optimization    Allowed           >
  Notification access     Allowed           >

  Received files location    Downloads/Cosync  >
  Connection details                           >
  Disconnect                                   >
  Forget this PC                               >
  About Cosync                                 >

[ "Your data stays between your paired devices." ]
```

**First launch flow**
```
App opens → "Connect your PC" (PC icon, Scan QR button)
→ Camera opens immediately (no permission carousel first)
→ QR viewfinder with corner brackets
→ Connecting screen ("Securing connection")
→ Connected Home
```
No onboarding carousel. No welcome tutorial.

**System surfaces to register**
- Android share sheet target ("Send to DESKTOP-ATELIER")
- Quick Settings tile (Connected/Disconnected, tap toggles, long-press opens app)
- Clipboard paste notification ("Copied from DESKTOP-ATELIER — tap to paste")
- Transfer progress notification (with Pause / Cancel)
- Transfer complete notification (with Open / Show in Files)
- Permission explanation modals (explain before Settings redirect, never dump
  the user into Settings silently)

---

## Platform Targets & Performance Contract

**Supported platforms:** Windows 10/11 and Linux (primary). Both must
work seamlessly and receive equal implementation attention. Native
platform-specific libraries (Win32 APIs, GTK, libnotify, etc.) are
only used when they provide a clear, measurable advantage in latency
or resource efficiency over the cross-platform Tauri/Rust path. Never
use a native-only API just because it's convenient — it must earn its
complexity by being faster or lighter.

**Performance is a first-class requirement, not a quality-of-life
concern.** The acceptance criteria for every milestone implicitly
includes: no unnecessary memory allocation in hot paths, no polling
where event-driven alternatives exist, no blocking the main thread,
no redundant re-renders. A feature that works but is sluggish or
resource-heavy is not done.

Specific targets:
- Clipboard sync end-to-end latency: < 200ms on local Wi-Fi
- File transfer throughput: within 80% of raw Wi-Fi throughput
  (QUIC overhead should be negligible)
- Desktop idle memory: < 50MB RSS after first run settles
- Android foreground service idle: < 5% battery drain per hour
  measured over a 4-hour soak
- UI frame budget: 60fps — no janky transitions, no blocking
  operations on the React render thread

When choosing between two implementation approaches, the faster and
lighter one wins unless there is a correctness reason to prefer the
other. Document the reason if you choose the slower path.

**Cross-platform implementation rules:**
- Write Rust logic once in `cosync-core` — never duplicate networking,
  crypto, or sync logic in platform-specific code
- Tauri abstracts the webview — use it; don't write Win32 or GTK UI
  directly unless profiling shows the webview is a bottleneck
- For system notifications: use `tauri-plugin-notification` which
  maps to WinRT toast on Windows and libnotify on Linux. Only drop
  to raw Win32/DBus if the plugin can't support a required feature
- For the system tray: Tauri's tray API is cross-platform — use it
- For clipboard hooks: Windows has `AddClipboardFormatListener`
  (event-driven, zero-cost idle); Linux has no equivalent so use
  a 250ms poll with X11/Wayland clipboard APIs via `arboard`. This
  is the one justified platform divergence in the clipboard path.
- For global shortcuts: `tauri-plugin-global-shortcut` handles both
- Test every feature on both platforms before marking a milestone done

---

## Architecture Summary

| Layer | Choice |
|---|---|
| Shared core logic | Rust (`cosync-core` crate) |
| Desktop shell | Tauri v2 + React/TypeScript |
| Mobile shell | React Native (Expo bare workflow) |
| Rust↔Mobile bridge | `uniffi-rs` → Kotlin bindings → RN native module |
| Discovery | `mdns-sd` crate (mDNS/DNS-SD) |
| Transport | QUIC via `quinn` + `rustls` (TLS 1.3, pinned self-signed certs) |
| Payload encoding | Protobuf via `prost` |
| Local storage | SQLite via `rusqlite` (bundled) |

---

## Status: What Is Already Done

```
Milestone 0 ✓  Repo scaffolding (Rust workspace, Tauri shell, RN/Expo bare)
Milestone 1 ✓  Protocol & data model (Envelope, HLC, DeviceIdentity, PairingPayload)
Milestone 2 ✓  Discovery & secure pairing (mDNS, QUIC mutual TLS, PairedDeviceStore)
Milestone 3    Rust backend ✓ (AppState, commands, tray, pairing listener)
               React frontend ✗ — this is the next thing to build
```

---

## Milestone 3 — Desktop UI: Tray + Pairing (Iterations 1 & 2)

**Goal:** Replace the default Tauri template with a real, working desktop
UI for the two most critical surfaces: the system tray and the pairing
window. Nothing more. Clipboard and settings come in Milestone 3b.

**Prerequisites:** Milestone 3 Rust backend (already complete).

**Iteration 1 — Functional (build this first)**

Wire the real backend data to minimal React components. It must work
correctly; it does not need to look good yet.

1. Delete the default `App.tsx` content. Create a minimal router that
   reads the `navigate` event from Tauri (the backend already emits this)
   and renders one of: `<PairingScreen>`, `<DeviceListScreen>`.

2. `<PairingScreen>`: call `get_pairing_qr` → parse the JSON → render the
   `public_key_fingerprint` and raw JSON payload as text. No QR rendering
   yet — just confirm the data arrives. Show a "Waiting for phone..." label.

3. `<DeviceListScreen>`: call `list_paired_devices` on mount, and re-call
   every 2 seconds. Render a plain `<ul>` of device names and connected
   booleans. When `paired-device-connected` fires, re-fetch and update.

4. Install `qrcode.react` (`npm install qrcode.react`). Replace the raw
   JSON text in `<PairingScreen>` with `<QRCodeSVG value={payload} />`.

5. Add `npm install @tauri-apps/plugin-global-shortcut` — register
   `Ctrl+Shift+V` globally in the Rust backend as a no-op for now
   (placeholder for Milestone 3b's clipboard history window).

**Acceptance criteria (Iteration 1):** Tray icon appears. Clicking
"Show pairing QR" opens a window that renders a scannable QR code
containing real pairing data from the backend. Clicking "Paired Devices"
shows the real device list. No crashes.

**Iteration 2 — Designed (do this second)**

Apply the design system. Don't change any wiring — only styling.

1. Set up CSS custom properties matching the design tokens above. Create
   `src/styles/tokens.css`. Import it once in `main.tsx`.

2. `<PairingWindow>` styled to match the pairing mockup (Image 18):
   - Dark `#1a1a1f` background, native window title "Cosync — pair"
   - "THIS PC" eyebrow label (10px, secondary text)
   - PC hostname in bold (pull from `window.__HOSTNAME__` injected by
     the Tauri backend via `tauri::command` returning `gethostname`)
   - QR code in a white-background rounded rectangle (`#ffffff`, 8px)
   - "Open Cosync on your phone and scan. Keys pin here — no account,
     no cloud." body copy
   - LAN row: Wi-Fi icon + `_cosync._udp.local` + copy-to-clipboard icon
   - "End-to-end encrypted local pairing" footer with shield icon

3. Tray menu already works from the Rust side. No React component needed
   for the tray menu itself — Tauri renders it natively. Confirm the
   tooltip reflects connection state using the three variants from the
   design (green dot/Connected, amber/Reconnecting, gray/Offline).

4. `<DeviceListScreen>` minimal styled variant — just a clean list with
   green/gray status dots. This screen is transitional; the real device
   management lives in Settings (Milestone 3c).

5. Set Tauri window config for the pairing window:
   - Width: 580, height: 620, resizable: false
   - `decorations: true` (native title bar)
   - Center on screen when opened

**Acceptance criteria (Iteration 2):** The pairing window looks like
Image 18. Tray tooltip updates correctly on connect/disconnect.

**Context Handoff for 3b:** Two real, styled windows exist. The backend
exposes `get_pairing_qr`, `list_paired_devices`, `get_connection_status`,
and emits `paired-device-connected` and `navigate` events. The global
shortcut `Ctrl+Shift+V` is registered but does nothing yet.

---

## Milestone 3b — Desktop UI: Clipboard History Window (Iterations 1 & 2)

**Goal:** The clipboard history launcher. This is a command-palette-style
popup, not a panel inside a larger window.

**Prerequisites:** Milestone 3 (pairing window working), Milestone 5
(clipboard sync working). Build the UI shell now but wire it to real
data only after Milestone 5.

**Iteration 1 — Functional**

1. Add a `get_clipboard_history` Tauri command (stubbed — returns 3–5
   hardcoded history items: text, a URL, an image filename). Wire the
   `Ctrl+Shift+V` global shortcut to open this window.

2. New Tauri window: `clipboard-history`. Width: 480, height: 480,
   frameless, `alwaysOnTop: true`, transparent background.

3. `<ClipboardHistoryWindow>`:
   - Text input, auto-focused on open
   - Scrollable list of history items (type icon, preview text, source
     device, timestamp)
   - Click item → invoke `restore_clipboard_item(id)` (stubbed for now)
   - Escape key closes the window

4. Basic keyboard navigation: up/down arrows move selection, Enter
   triggers restore on the selected item.

**Iteration 2 — Designed**

Apply the design to match the clipboard history mockup from the spec:

```
Clipboard history · Ctrl+Shift+V

[ Search... ]                           ← auto-focused, full-width

T  https://notes.local/brief/q3-sync    ← type icon (link, text, image, code)
   This PC · 4m · Link

⌨  Gate code 4821...
   Pixel 8 · 3h · Text

<> const sync = await...
   This PC · 5h · Code

────────────────────────────────
● Pixel 8 · connected
Enter to restore
```

- Frameless window with subtle border (1px `rgba(255,255,255,0.1)`)
- `#1a1a1f` background, `rgba(0,0,0,0.6)` window-level backdrop blur
  if the platform supports it (Windows 11 Mica or Acrylic via Tauri)
- Selected row: `rgba(59,130,246,0.15)` background + left blue accent
  border `2px solid #3b82f6`
- Footer bar: status dot + device name + "Enter to restore" dim hint

**Acceptance criteria (Iteration 2):** Window opens on shortcut, is
keyboard navigable, closes on Escape, looks like the design. Content
is still stubbed — real sync data wires in after Milestone 5.

---

## Milestone 3c — Desktop UI: Settings Window (Iterations 1 & 2)

**Goal:** The Settings window — the only traditional full-window surface
in the desktop app. Sidebar navigation with 9 panels.

**Prerequisites:** Milestone 3 complete.

**Iteration 1 — Functional**

1. New Tauri window: `settings`. Width: 740, height: 560, resizable.
   Open from tray "Settings" menu item.

2. Sidebar: General, Clipboard, Files, Notifications, Connection,
   Appearance, Advanced, About. **No separate Devices tab** — device
   management lives in General. Clicking switches the content panel.
   State is in React only — no URL routing needed.

3. Implement panels in this order (block on later milestones for
   content that isn't built yet):
   - **General**: Two sections in one panel:
     *App behavior* — "Start at login" toggle (Tauri autostart
     plugin), "Keep in tray", "Show connection notifications",
     "Update channel" dropdown. Wire all to a `get_settings` /
     `set_setting(key, value)` Tauri command pair backed by a
     `settings` SQLite table.
     *Paired devices* — list of paired devices with green/gray
     status dots and a "…" overflow menu per device (Disconnect,
     Forget). "Pair another device" button at bottom opens the
     pairing window. This is the only place device management lives.
   - **Clipboard**: stubbed toggles and inputs — wire to real state in
     Milestone 5 / 7.
   - **Files**: stubbed — wire in Milestone 6.
   - **Notifications**: stubbed — wire in Milestone 8.
   - **Connection**: auto-reconnect toggle, device discovery toggle,
     network interface dropdown (Automatic / list of interfaces).
     Collapsible "Advanced connection information" section showing
     Local IP, mDNS service name, Transport (QUIC), TLS version.
   - **Appearance**: System / Light / Dark radio group. Applies a CSS
     `data-theme` attribute to `<html>`. Default: System.
   - **Advanced**: Export logs, Open data folder, Reset settings, Clear
     local data, Unpair all devices.
   - **About**: App version + "Check for updates" button, Open-source
     licenses link, Privacy link, GitHub link, Report issue link.
     Footer: "Cosync works over your local network. Nothing is
     uploaded to a server."

4. Status bar at sidebar bottom: "● Cosync is running · v1.0.0"
   (version pulled from `tauri::VERSION`).

**Iteration 2 — Designed**

Match Images 11, 12, 13, 19 with the following corrections:
- Two-column layout: 200px sidebar left, content panel right
- Sidebar: 8 items (General, Clipboard, Files, Notifications,
  Connection, Appearance, Advanced, About). No Devices entry.
- Sidebar item: 16px icon + label, selected state is a filled rounded
  rect `rgba(255,255,255,0.06)` with blue left accent
- Content panel: white-space generous, 400px max-width card per section,
  each row is label + description (12px secondary) + control
- Toggle rows: label left, native-looking switch right
- Dropdown: system-styled select or custom component matching OS
- Destructive rows (Reset, Unpair all, Forget device): red text
- Device list item (in General panel): generic phone icon (Fluent
  UI smartphone outline, 20px) + name + status dot + overflow button.
  **No device photo, no model-specific image** — it would require
  a lookup service which contradicts the local-first, no-cloud model.
  The generic phone icon is sufficient and always correct.

**Acceptance criteria (Iteration 2):** Every panel renders without
crashing. Appearance toggle actually changes the theme. Device list
reflects real paired-device data. All other panels render correctly
styled with stubs where backend isn't ready yet.

---

## Milestone 4 — Android Client Shell

**Goal:** A real Android app wired to the same Rust core, with the
mandatory foreground service in place before any feature work.

**Prerequisites:** Milestones 0–2 complete.

**Iteration 1 — Functional (bare minimum first)**

1. Cross-compile `cosync-core` for Android:
   ```
   cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 build --release
   ```
   Copy `.so` files to `apps/mobile/android/rust-bridge/src/main/jniLibs/<abi>/`.

2. Run `uniffi-bindgen generate --language kotlin` against the
   `cosync-core` UDL; place generated Kotlin under
   `rust-bridge/src/main/java/com/cosync/rustbridge/generated/`.

3. Write `CosyncBridgeModule.kt` (extends `ReactContextBaseJavaModule`).
   Expose only: `startDiscovery()`, `getPairingPayload()`,
   `pairWithScannedPayload(json)`, plus an event emitter for connection
   state changes. Register in `MainApplication.kt`.

4. Implement `CosyncForegroundService.kt`. It must start before any
   discovery call and show a persistent "Cosync is active" notification.
   This is non-negotiable — do it now, not later.

5. On the TypeScript side: a typed `NativeCosync` module wrapper and a
   `useCosyncConnection()` hook.

6. Add manifest entries: `FOREGROUND_SERVICE`, `CHANGE_WIFI_MULTICAST_STATE`,
   `CAMERA`, `INTERNET`, `ACCESS_NETWORK_STATE`, foreground service type
   declaration.

7. Minimal RN UI — just two screens: `<FirstLaunchScreen>` (bare button
   that starts the camera) and `<HomeScreen>` (shows "Connected" or
   "Not connected"). No design yet.

**Acceptance criteria (Iteration 1):** Install on device → foreground
notification appears → pairing scan works → status screen shows
"Connected." Killing and reopening reconnects automatically.

**Iteration 2 — Designed**

Apply the mobile design language. Build in this screen order:

**First launch** (Image: Screenshot 10:13:30)
- Full-screen dark `#0f1117` background
- Monitor icon centered
- "Connect your PC" heading
- "Open Cosync on your computer and show the pairing QR." body
- "Scan QR" CTA button — full width, blue, rounded, bottom of screen
- No carousel, no skip button, no dots

**QR Scanner** (Image: Screenshot 10:13:40)
- Full-screen camera view
- "Cancel" top left
- Corner bracket viewfinder overlay (4 rounded L-shapes, white strokes)
- "Point at the QR on DESKTOP-ATELIER" label below the brackets

**Connecting** (Image: Screenshot 10:13:49)
- Full screen, centered content
- "CONNECTING" eyebrow label
- "Pixel 8 ↔ DESKTOP-ATELIER" with a right-arrow between
- "Securing connection" subtitle
- No spinner — the arrow animation is enough

**Home — initial state** (Image: Screenshot 10:14:13, adjusted per
Sani's instruction)
- App bar: Cosync logo icon + "Cosync" label + overflow ⋮
- **Connection card — large, ~45–50% of initial viewport:**
  - PC monitor icon (large, ~64px)
  - Device name ("DESKTOP-ATELIER") large bold
  - `● Connected` status line
  - "Wi-Fi · Local network" secondary line
  - NO clipboard/files/notifications toggle cards here — these are
    on the Control page only. The connection state fills this space.
- Clipboard section header
- Search clipboard input
- List of clipboard history items (icon + preview + source + time + ⋮)
- Bottom nav: Home (active) | Control

**Home — scrolled state** (Image: Screenshot 10:14:28)
- Animated collapse of connection card to:
  ```
  DESKTOP-ATELIER  ● Connected · Wi-Fi · Local network
  ```
  (one line, left-aligned under app bar)
- Clipboard list fills ~80% of the screen
- Bottom nav remains visible

**Control page** (Image: Screenshot 10:14:41 / 10:15:18)
- Compact connection card at top
- "ALLOW THIS PC TO ACCESS" section with three toggles:
  Clipboard, Files, Notifications — each with subtitle
- "SYSTEM STATUS" section:
  Background access (→ opens app's battery settings),
  Battery optimization (→ opens battery settings),
  Notification access (→ opens notification access settings)
- Received files location (→ folder picker)
- Connection details (→ bottom sheet)
- Disconnect
- Forget this PC (red destructive)
- About Cosync
- Footer: "Your data stays between your paired devices."

**Overflow menu** (⋮ in app bar)
- Connection details
- Received files
- Pause connection
- Disconnect
- Forget this PC
- About Cosync

**Connection details bottom sheet** (Image: Screenshot 10:15:28)
- "Connection details" title
- Paired device: DESKTOP-ATELIER
- Connection: ● Connected
- Network: Wi-Fi · Local network
- Latency: 14 ms
- Last sync: Just now
- Trusted device: Yes
- "Copy device ID" button + "Close"

**Permission explanation screens** (Image: Screenshot 10:15:18)
- Back button top left
- Large permission icon (64px, circle background)
- Permission name heading
- Two-sentence explanation of why this is needed and what stays private
- "Allow access" full-width blue button
- Never navigate to Android Settings without showing this screen first

**Quick Settings tile and notification style**: implement as Android
native Kotlin — not React Native. `CosyncTileService.kt` for the
QS tile, notification styles in `CosyncForegroundService.kt`.

**Acceptance criteria (Iteration 2):** Every screen looks like its
reference image. Home page has no trust toggles. Connection card is
large. Scrolling collapses it smoothly. Control page has the toggles.
Permission screens explain before redirecting to system settings.
QS tile shows Connected/Disconnected state.

---

## Milestone 5 — Clipboard Sync (Core Feature)

**Goal:** Copy on phone → paste on PC. Copy on PC → paste notification
on phone. Bidirectional, real-time, conflict-free.

**Prerequisites:** Milestones 3 and 4 (both shells exist and paired).

**Iteration 1 — Functional**

1. Desktop: `arboard` crate, native clipboard change hook
   (`AddClipboardFormatListener` on Windows, 250ms poll elsewhere).
   On change, wrap in `Envelope::ClipboardUpdate` with HLC tag and
   `source_device_id`. Send over the QUIC session.

2. Apply `should_apply_update` (HLC causal ordering + loop prevention)
   on receive before touching the local clipboard.

3. Android: `ClipboardManager.OnPrimaryClipChangedListener` inside the
   foreground service. Route through the same `Envelope` path.

4. Android receive → heads-up notification "Copied from DESKTOP-ATELIER
   — tap to paste." Tapping fires `ClipboardManager.setPrimaryClip`.
   This is required on Android 12+ because background clipboard writes
   are blocked without visible user acknowledgment.

5. 5MB hard cap on clipboard payloads. Reject oversized items silently
   on sender side; log to debug output.

**Acceptance criteria (Iteration 1):** Plain text copied on phone
appears in PC clipboard within 2 seconds. Plain text copied on PC
produces a paste notification on phone that works when tapped. Rapid
alternating copies don't loop.

**Iteration 2 — Designed**

1. Desktop notification style for incoming clipboard item: small,
   no action buttons needed (the item is already in the clipboard).

2. Android notification style matches the design: "Cosync · Copied from
   DESKTOP-ATELIER" title, content preview (truncated), "Paste" action
   button, Cosync icon.

3. Wire the real clipboard history list into `<ClipboardHistoryWindow>`
   (desktop) — replace stubs from Milestone 3b with real `cosync-core`
   data.

4. Wire the real clipboard history list into the Home page (mobile).

**Iteration 3 — Polish**

- Image clipboard support (PNG/JPEG)
- URL type detection (renders link icon in history)
- Code detection heuristic (renders code icon, monospace preview)

---

## Milestone 6 — File Transfer

**Goal:** Drag file from desktop to phone, or share file from Android
to desktop. Chunked, resumable, checksum-verified.

**Prerequisites:** Milestone 5 (proves the Envelope+QUIC-stream pattern).

**Iteration 1 — Functional**

1. Rust: `FileMeta` message first (name, size, SHA-256, chunk count),
   then ordered `FileChunk` messages (64KB each) on a dedicated QUIC
   stream (multiplexed — doesn't block clipboard or heartbeat traffic).

2. Desktop sender: Tauri drag-drop event → stream `std::fs` file in
   chunks. Desktop receiver: write to temp file, verify checksum,
   move to `Downloads/Cosync`.

3. Android sender: Storage Access Framework content-URI → stream to
   Rust bridge → send. Android receiver: write chunks → verify → insert
   into MediaStore so the file appears in Files/Gallery.

4. Resume support: persist the last acknowledged chunk index; on
   reconnect, resume from that offset rather than starting over.

**Acceptance criteria (Iteration 1):** A 100MB file dragged from desktop
arrives on the phone intact (checksum matches, visible in gallery).
A file shared from Android appears in `Downloads/Cosync` on desktop.
Dropping Wi-Fi mid-transfer and reconnecting resumes.

**Iteration 2 — Designed**

Desktop tray: when a transfer is active, show the inline progress row
above the menu items (Image 16 — with active transfer):
```
↓ vacation.mp4            78%
──────────────────────────
[rest of menu]
```

Desktop transfer completion notification (Image 14, lower section):
- "photo.jpg received · From Pixel 8 · just now"
- "Open" and "Show in folder" action buttons
- Small, no large Cosync branding

Android transfer progress notification (matching the design in doc):
- "Receiving vacation.mp4 from DESKTOP-ATELIER"
- Progress bar, "612 MB / 783 MB"
- Pause / Cancel actions

Android transfer complete notification:
- "vacation.mp4 received"
- Open / Show in Files actions

**Iteration 3 — Polish**
- Send file from tray "Send file…" item (opens file picker)
- Folder transfer (zip transparently, unzip on receive)
- Transfer queue (multiple files in sequence)

---

## Milestone 7 — Clipboard History & Search

**Goal:** Persistent, searchable clipboard history on both devices.

**Prerequisites:** Milestone 5 (clipboard sync working).

**Iteration 1 — Functional**

1. `clipboard_history` SQLite table: `id`, `content`, `content_type`,
   `source_device_id`, `hlc_time`, `created_at`. Cap at 100 items
   (oldest-first eviction).

2. Every accepted `ClipboardUpdate` appended to the table.

3. Expose `get_history(query)` and `restore_history_item(id)` through
   Tauri commands and the Android bridge.

4. Restoring an old item re-tags it with the current HLC timestamp and
   re-syncs it as a fresh clipboard update — never re-broadcasts a stale
   timestamp.

**Iteration 2 — Designed**

- Desktop: real data wired into the clipboard history window from
  Milestone 3b. Fuzzy search using `fuzzy-matcher` crate. Type icons
  (text `T`, image 🖼, link `🔗`, code `<>`).

- Mobile: search field on Home page searches local history. Each item
  shows type icon + preview + source device + time. Tapping an item
  opens a native Android bottom sheet:
  `Copy | Send to PC | Delete`

---

## Milestone 8 — Notification Mirroring & Quick Replies

**Goal:** Phone notifications appear on desktop. Supported
notifications can be replied to from the desktop keyboard.

**Prerequisites:** Milestone 4 complete (foreground service stable).
Best started after Milestones 5–7 are stable.

**Iteration 1 — Functional**

1. `NotificationListenerService` on Android. User grants permission via
   the Control page Notifications toggle → permission explanation screen
   → Android notification access settings.

2. On `onNotificationPosted`: extract app name, title, body, package,
   notification ID. Check `notification.actions` for `RemoteInput`
   capability — set `has_reply: true` if present.

3. New proto message `NotificationEvent`; send to desktop.

4. Desktop: fire a native OS notification (via `tauri-plugin-notification`)
   with the source app's name and the phone identifier as suffix
   ("WhatsApp · Pixel 8"). If `has_reply`, add the reply as a native
   OS notification action button — **no custom Cosync reply window**.
   On Windows this uses WinRT toast actions; on Linux it uses
   libnotify action buttons. Both are native and require zero custom
   UI. The trade-off: on Windows, notification action buttons only
   appear when the user has "Show notification actions" enabled (it
   is on by default in Windows 10/11). This is intentional — the
   feature is opt-in at the OS level and we don't build around it.
   Document this clearly in Settings → Notifications: "Quick replies
   use native notification actions. If buttons don't appear, check
   Windows notification settings."

5. Reply path: user types in the OS notification reply field → desktop
   sends `NotificationReply(id, text)` → phone's listener retrieves
   the `StatusBarNotification` by ID and fires the `RemoteInput`
   `PendingIntent`. Fail gracefully if the notification was already
   dismissed (show brief error toast, no crash).

6. Allow/deny list by package name, configurable in Settings →
   Notifications (default: all off, user enables per-app).

**Acceptance criteria (Iteration 1):** WhatsApp message on phone
appears as desktop notification within ~1 second. Reply from desktop
delivers via WhatsApp on the phone.

**Iteration 2 — Designed**

Match Image 14 (notification mockup) exactly:
- "WhatsApp · Pixel 8" header with WhatsApp app icon
- Message preview
- Inline reply field + Send button (when `has_reply`)
- Telegram example shows no reply field (body only) when
  `has_reply` is false
- No large Cosync banner — the originating app's icon is the identity

---

## Milestone 9 — Production Hardening

**Goal:** A daily-drivable, trustworthy background service.

**Prerequisites:** Milestones 5–8 complete.

1. Sentry crash reporting in Tauri (Rust + JS) and Android (Kotlin + RN).
   **Never log clipboard content.** Scrub before sending.

2. Tauri auto-updater wired to GitHub Releases feed.

3. Stress test reconnection: Wi-Fi toggle, band switch (2.4/5GHz),
   sleep/wake, device reboot — multi-hour soak.

4. Memory leak audit: `heaptrack` on desktop, Android Studio Memory
   Profiler on mobile. Fix any un-dropped `Arc<Mutex<_>>` cycles or
   unclosed file handles.

5. Windows code-signing certificate. Play Store submission materials
   including data-safety form and justification copy for
   `NotificationListenerService` (expect at least one review round-trip).

6. Auto-start on login: Tauri autostart plugin, wired to the
   "Start at login" toggle in Settings → General.

**Acceptance criteria:** Multi-hour unattended soak test passes.
Installs without security warnings on Windows. Play Store review
materials drafted.

---

## Milestone 10 — Deferred (v2.0): Virtual Webcam, Mic, SMS/Calls

**Status:** Intentionally under-specified. Don't start this before
Milestone 9 is fully done and you've used the app as a daily driver.

- **Virtual webcam:** CameraX → MediaCodec H.264 → QUIC → PC decode
  → OBS Virtual Camera plugin API (don't write a DirectShow filter
  from scratch).
- **Virtual mic:** same pipeline, Opus codec.
- **SMS/Calls:** requires being set as default SMS app — validate Play
  Store approvability before investing engineering time.

---

## Iteration Checklist (use per milestone)

Before marking any milestone done, verify every item below. A milestone
is not done if any of these fail.

**Correctness**
- [ ] Works with a real paired device (not just the test suite)
- [ ] No regressions in previously-completed milestones
- [ ] Error states are handled visibly (no silent failures)
- [ ] No hardcoded strings that should be dynamic

**Cross-platform**
- [ ] Tested on Windows 10 or 11
- [ ] Tested on Linux (Ubuntu 22.04+ or equivalent)
- [ ] No Windows-only or Linux-only code paths except where explicitly
      documented with a performance justification (clipboard hooks are
      the single allowed exception)
- [ ] System notifications fire correctly on both platforms

**Design**
- [ ] UI matches the design system reference above
- [ ] Keyboard-navigable on desktop where applicable
- [ ] Settings sidebar has 8 items (no Devices tab)
- [ ] Notification replies use native OS action buttons only
- [ ] Device list shows generic phone icon, no photos

**Performance**
- [ ] No polling loops where event-driven alternatives exist
- [ ] No blocking operations on the main/render thread
- [ ] Clipboard sync latency < 200ms measured on local Wi-Fi
- [ ] Desktop idle memory has not regressed from the previous milestone
- [ ] No unnecessary React re-renders on clipboard/status events
      (use `React.memo`, `useMemo`, or `useCallback` where profiling
      shows it matters — not by default everywhere)

**Privacy & security**
- [ ] Does not log, store, or transmit clipboard content in any
      error report or crash payload
- [ ] No personal data leaves the LAN

