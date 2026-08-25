# Cosync — Development Specification (v3, Agile / Human-Gated)

> **Normative status:** This file is the authoritative implementation contract for Cosync.
>
> If this file conflicts with an older instruction in `AGENTS.md`, `docs/DECISIONS.md`,
> an earlier SPEC, a mockup, or an agent's own plan, **this SPEC wins** until the
> conflicting document is explicitly updated.
>
> The product direction is stable; the implementation is deliberately incremental.

---

## 0. Non-Negotiable Development Rule

### DO NOT BUILD COSYNC ALL AT ONCE

Cosync is built as a sequence of **small, runnable, manually verified vertical slices**.

The goal is not to make the whole product look finished before it is useful.

The goal is:

1. build the smallest real feature;
2. run it on real devices;
3. let the developer test it manually;
4. fix what is wrong;
5. only then move forward;
6. continuously improve the same working product until it becomes the complete product described in this document.

A feature that exists only in mocks, hardcoded data, dead UI, or disconnected scaffolding is **not progress** unless the current iteration explicitly requires a temporary shell.

### Mandatory human verification gate

Every iteration ends at a human gate.

After completing **one iteration only**, the agent must:

1. build the runnable artifact(s) for the platforms involved;
2. run automated tests, lint/type checks, and build checks;
3. state exactly what was implemented;
4. list every file materially changed;
5. provide the exact command(s) required to run the build;
6. provide a short manual verification checklist;
7. report known limitations and any performance numbers measured;
8. mark the state as `READY_FOR_HUMAN_TEST`;
9. **STOP AND WAIT**.

The agent must **not**:

- start the next iteration;
- start the next milestone;
- pre-build a future feature;
- add "while we are here" improvements;
- turn placeholder UI into unrelated functionality;
- infer approval because automated tests passed;
- mark a human acceptance criterion as passed by itself.

An iteration becomes `APPROVED` only when the developer explicitly confirms that
the runnable build was manually tested and approved, or explicitly instructs the
agent to proceed despite a known failed criterion.

The default after finishing any iteration is:

> **STOP. WAIT FOR HUMAN VERIFICATION.**

### What "Acceptance Criteria pass" means

Acceptance Criteria pass only when **both** are true:

- automated/build criteria pass; and
- the developer has manually tested the runnable build and explicitly approved it.

---

## 1. Agile Execution Model

Every feature is developed in up to three passes.

```text
Iteration 1 — Functional
The feature works for real.
Wiring is real.
UI is only as complete as necessary to test it.
No speculative polish.

        ↓ HUMAN TEST / APPROVAL

Iteration 2 — Designed
The proven feature is shaped to match the final native-feeling UI/UX.
No architecture rewrite unless testing exposed a real problem.

        ↓ HUMAN TEST / APPROVAL

Iteration 3 — Polished
Keyboard behavior, accessibility, animation, edge cases, resilience,
performance tuning, and optional enhancements.

        ↓ HUMAN TEST / APPROVAL
```

Not every milestone needs all three iterations immediately.

**Iteration 3 is normally deferred** until the feature has been used enough to justify
polish.

### Vertical-slice rule

Prefer a complete end-to-end slice over building horizontal layers.

Good:

```text
desktop pairing UI
→ Android pairing
→ real connection
→ manually test
→ real clipboard handoff
→ manually test
```

Bad:

```text
all desktop windows
→ all mobile screens
→ all settings
→ all animations
→ eventually connect them to the backend
```

### No future-feature stubs by default

Do not build panels for Files, Notifications, Clipboard History, etc. before the
corresponding feature exists unless the current iteration explicitly needs the shell.

Settings and Control surfaces grow **with the product**:

- pairing exists → device/connection controls become real;
- clipboard exists → clipboard controls become real;
- file transfer exists → file settings become real;
- notification mirroring exists → notification controls become real.

No large collection of fake switches or hardcoded future state.

---

## 2. Release Strategy

Cosync must always be capable of producing a small usable build.

### Release checkpoint R0.1 — Connected devices

Minimum:

- desktop tray runs;
- desktop can show a real pairing QR;
- Android can scan and pair;
- pinned certificate trust is used;
- both sides reconnect after restart;
- connection state is visible;
- Windows and Linux desktop paths are both verified.

This is a working connectivity preview.

### Release checkpoint R0.2 — Minimum useful release

Add:

- PC → Android clipboard handoff;
- Android → PC text handoff through a reliable Android-supported action;
- no clipboard loops;
- low-latency LAN transport;
- minimal connection/trust controls.

This is the first release that provides daily utility.

### Later checkpoints

```text
R0.3  Persistent clipboard history + search
R0.4  File transfer
R0.5  Notification mirroring
R0.6  Quick replies where the OS permits
R1.0  Hardening, installers, updates, policy review, soak tests
```

The version numbers above are product checkpoints, not a requirement for Git tags.
Use the project's actual versioning workflow.

---

## 3. Product Definition

Cosync is a **local-first bridge between an Android phone and a Windows/Linux PC**.

Its job is to make selected device capabilities feel like they belong to the same
personal computing environment:

- clipboard handoff/sync;
- clipboard history;
- file transfer;
- Android notification mirroring;
- supported notification replies;
- persistent trusted-device connectivity.

Cosync is not a cloud storage product, account system, dashboard, or productivity suite.

### Product principle

> **Cosync is infrastructure with a UI, not an app with infrastructure.**

The user should spend very little time "inside Cosync."

The product should appear in the operating-system surface where an action naturally
belongs:

- connection state → tray / Quick Settings;
- pairing → small pairing window / QR scanner;
- desktop clipboard history → global shortcut palette;
- Android send → Android share sheet;
- transfer progress → compact transfer utility + OS notifications;
- mirrored phone notifications → desktop OS notifications;
- configuration → Settings / Control;
- received files → the system file manager.

---

## 4. Decisions Already Made

These decisions are preserved from the earlier specification and architecture records.

- Shared networking/sync/crypto core is Rust (`cosync-core`).
- Desktop shell is Tauri v2 + React/TypeScript.
- Mobile shell is React Native / Expo **bare workflow**, not Flutter.
- Rust↔Android bridge uses UniFFI-generated Kotlin bindings wrapped by a thin RN module.
- Discovery uses mDNS/DNS-SD.
- Transport uses QUIC (`quinn`) with `rustls` / TLS 1.3.
- Trust is pairwise certificate fingerprint pinning using self-signed X.509 certificates.
- Payload metadata/control messages use Protobuf via `prost`.
- Local persistence uses SQLite via `rusqlite`.
- Core device-to-device content does not use a cloud relay.
- macOS is out of scope.
- Windows 10/11 **and Linux are both first-class desktop targets**.
- Windows may be the primary development host, but a desktop iteration cannot be
  accepted until the same behavior is verified on Linux.
- iOS is out of the current roadmap unless real demand appears.
- Virtual webcam, virtual mic, SMS and calls remain deferred to v2.0.
- LocalSend was studied but Cosync is not a LocalSend fork.
- Pinned trust uses the SHA-256 fingerprint of the self-signed TLS certificate,
  not the raw Ed25519 identity key.
- Existing old crate version pins caused by an old build environment are not
  architectural requirements. Use current stable Rust via `rustup` and update
  dependencies carefully on the developer machine.

### Important correction to older ADR language

Any older statement that calls Linux "secondary" is superseded.

**Current rule:** Windows and Linux are both first-class product targets.

Platform-specific implementations are permitted when they provide a measurable
advantage in:

- latency;
- idle resource usage;
- reliability;
- OS integration that cannot be achieved correctly cross-platform.

Any non-trivial platform divergence must be documented.

---

## 5. Current Repository Status

At the time this specification was rebuilt:

```text
Milestone 0 ✓
Repo scaffolding:
- Rust workspace
- Tauri shell
- RN/Expo bare project

Milestone 1 ✓
Protocol & data model:
- Envelope
- HybridLogicalClock
- DeviceIdentity
- PairingPayload

Milestone 2 ✓
Discovery & secure pairing core:
- mDNS
- QUIC
- mutual TLS
- pinned certificate trust
- PairedDeviceStore

Desktop Rust backend ✓
- AppState
- Tauri commands
- tray plumbing
- pairing listener

Desktop React frontend ✗
- this is the next development surface
```

Do not rebuild completed milestones unless testing exposes a real defect.

---

## 6. Repository Layout

```text
cosync/
├── Cargo.toml
├── crates/
│   └── core/
│       └── cosync-core
├── apps/
│   ├── desktop/
│   │   ├── src/
│   │   └── src-tauri/
│   └── mobile/
│       ├── src/
│       └── android/
│           └── rust-bridge/
├── docs/
│   ├── SPEC.md
│   └── DECISIONS.md
├── AGENTS.md
└── CHANGELOG.md
```

Shared networking, crypto, protocol ordering, trust, transfer state and sync rules
belong in `cosync-core` wherever technically reasonable.

Platform shells should be thin adapters around the shared core.

---

# PART I — FINAL UX TARGET

The final UI target is described here so the product can evolve toward one coherent
destination without being built all at once.

---

## 7. Desktop UX Model

### 7.1 Desktop mental model

Desktop Cosync is **tray-first**.

There is **no conventional home/dashboard window**.

The user reaches features directly through:

- tray menu;
- global shortcut;
- compact utility windows;
- native OS notifications;
- system file picker/file explorer;
- Settings.

### 7.2 Final desktop surface inventory

Only these true custom surfaces are expected:

1. System tray/menu
2. Pairing window
3. Clipboard History palette
4. File Transfers utility window/popover
5. Device Details utility window/popover
6. Settings window

Everything else should use the OS where possible.

Do not add a dashboard, notification-center page, recent-activity page, internal
file browser, or large device-management page.

---

## 8. Desktop Design Language

### Window treatment

- Native OS decorations where the window is a normal desktop window.
- Standard platform min/max/close behavior where appropriate.
- Native desktop proportions; no full-screen feature windows.
- `8px` custom corner radius where custom rounding is required.
- Dark graphite base: approximately `#1a1a1f`.
- Slightly lighter surfaces: approximately `#222228`.
- Low-contrast border: `1px solid rgba(255,255,255,0.08)`.
- No decorative gradients.
- No oversized shadows.
- Use native system theme support; dark references are the primary design reference,
  not a requirement to ignore light/system mode.

### Typography

System-first stack:

```css
font-family: "Segoe UI", Inter, "Ubuntu Sans", system-ui, sans-serif;
```

Guidelines:

- inline text: roughly 12–14px;
- base compact UI: roughly 11–13px;
- eyebrows/metadata: roughly 10px;
- hero/device label: up to roughly 20px;
- monospace only for fingerprints, IDs, shortcuts, URLs/code where useful.

Hierarchy comes from weight/size/spacing, not decorative color.

### Color

- Surface: `#1a1a1f`
- Panel: `#222228`
- Primary text: `rgba(255,255,255,0.87)`
- Secondary text: `rgba(255,255,255,0.45)`
- Accent blue: `#3b82f6`
- Connected dot: `#22c55e`
- Reconnecting dot: `#f59e0b`
- Offline dot: subdued gray
- Destructive only: `#ef4444`

Use blue only for active controls, selection and progress.

Never turn connection state into a giant colored badge.

### Icons

- Fluent / Windows 11-like line icons are the reference language.
- Thin outline, rounded joins.
- 16px normal.
- Icons support labels; do not replace clear labels.

### Interaction

- keyboard-first on desktop launchers/lists;
- arrow keys + Enter where a list is actionable;
- Escape closes transient palettes;
- native switches/dropdowns/context menus;
- no spinner unless a genuine operation exceeds ~300ms and feedback is needed.

### Hard visual rules

Do not build:

- dashboard cards;
- analytics;
- recent-activity panels;
- SaaS navigation;
- giant CTAs;
- feature tiles;
- decorative gradients;
- mobile layouts stretched onto desktop;
- app-store-like sections;
- large device photos.

---

## 9. Desktop Tray

The tray is the desktop control center.

Final shape:

```text
Cosync
────────────────────────

● Pixel 8
  Connected

Show pairing QR
Clipboard history      Ctrl+Shift+V
Send file...
Transfers (2)          ← only shown when useful
Open received folder

────────────────────────
Settings
Quit
```

Disconnected:

```text
○ Pixel 8
  Offline
```

Reconnecting:

```text
●/amber Pixel 8
Reconnecting...
```

Rules:

- clicking the device row opens Device Details;
- `Transfers (n)` opens the File Transfers window;
- do **not** attempt to place a custom progress bar inside the native tray menu;
- tray menu items must remain native Tauri menu items.

---

## 10. Desktop Pairing Window

A small, focused window.

Target structure:

```text
Cosync — pair

THIS PC

DESKTOP-ATELIER

[ QR CODE ]

Open Cosync on your phone and scan.
Keys pin here — no account, no cloud.

LAN · _cosync._udp.local

End-to-end encrypted local pairing
```

Rules:

- show the real computer hostname;
- obtain hostname via a real Tauri `invoke("get_hostname")` command or equivalent,
  not an imaginary automatically injected browser global;
- QR payload is real pairing data;
- white QR background;
- native title bar;
- centered when opened;
- no onboarding wizard;
- no fake progress sequence.

Recommended size:

```text
width: ~580px
height: ~620px
resizable: false
```

---

## 11. Desktop Clipboard History Palette

Global shortcut default:

```text
Ctrl+Shift+V
```

Target:

```text
Clipboard history · Ctrl+Shift+V

[ Search... ]

T   https://notes.local/brief/q3-sync
    This PC · 4m · Link

T   Gate code 4821...
    Pixel 8 · 3h · Text

<>  const sync = await...
    This PC · 5h · Code

────────────────────────────────
● Pixel 8 · connected
Enter to restore
```

Behavior:

- opens immediately;
- search field auto-focuses;
- up/down changes selection;
- Enter restores selected item;
- Escape closes;
- click restores;
- list is scrollable;
- fuzzy search comes only when persistent history exists.

Do not build persistent history before the history milestone.

---

## 12. Desktop File Transfers Utility

This is a real compact utility window/popover.

Do not replace it with a dashboard and do not attempt to draw arbitrary widgets
inside the native tray menu.

Target:

```text
File transfers

[ Send file ]   [ Drop files here or click Send file ]

vacation.mp4                              78%
Sending to Pixel 8
████████████████░░░░
612 MB / 783 MB · 24.6 MB/s · 5s left
                                  Pause  Cancel

document.pdf                              12%
Receiving from Pixel 8
███░░░░░░░░░░░░░░░░
1.8 MB / 15.2 MB · 8.5 MB/s

photo.jpg
Sent to Pixel 8 · Completed             Open folder

────────────────────────────────────────────
● Pixel 8 · connected
Downloads/Cosync
```

Functions:

- send file picker;
- drag/drop desktop files;
- transfer list;
- progress;
- bytes;
- speed;
- ETA where meaningful;
- pause/cancel if the transfer implementation supports it;
- completion state;
- open folder.

OS notifications complement this window; they do not replace the window.

---

## 13. Desktop Device Details

Compact device inspector opened from tray or Settings.

Target information:

```text
Pixel 8
● Connected

Wi-Fi
Atelier Wi-Fi

Latency
12 ms

Last sync
Just now

Features
Clipboard       On
Files           On
Notifications   On

Trusted device
End-to-end encrypted

Open received folder
Disconnect
Forget device

Device ID / certificate fingerprint
[ copy ]
```

Rules:

- generic phone icon, not a fetched model image;
- no internet lookup;
- feature rows appear only once those features actually exist;
- Forget is destructive/red;
- certificate/fingerprint details can be tucked behind "Trusted device" if needed.

---

## 14. Desktop Settings

Settings is the only conventional multi-section desktop window.

Final sidebar has exactly **8 sections**:

```text
General
Clipboard
Files
Notifications
Connection
Appearance
Advanced
About
```

No separate Devices tab.

Device management lives in General.

### Incremental build rule

Do **not** build all eight panels as fake UI at once.

The Settings window grows as features become real.

Example:

```text
Pairing exists:
- General
- Connection
- Appearance
- About

Clipboard exists:
+ Clipboard

Files exists:
+ Files

Notifications exist:
+ Notifications

Hardening:
+ Advanced
```

The final sidebar still contains all eight sections once the product is complete.

### Final General

App behavior:

- Start Cosync when I sign in
- Keep Cosync running in system tray
- Reconnect automatically
- Show connection changes as notifications
- Update channel

Paired devices:

```text
Pixel 8
● Connected                 …

Old Pixel 6
○ Offline                   …

[ Pair another device ]
```

Menu:

- Device details
- Disconnect
- Forget

### Final Clipboard

- Clipboard enabled
- Keep clipboard history
- History limit
- Global shortcut
- Maximum item size
- Clear history
- Android clipboard capability explanation

### Final Files

- Received folder
- Auto-save / ask
- Resume interrupted transfers
- Transfer completion notifications
- conflict behavior
- Open received folder

### Final Notifications

- Mirror Android notifications
- Quick replies where supported
- notification sound
- respect OS Do Not Disturb
- per-app allow/deny list

Default app mirroring state should be conservative. Prefer opt-in per application.

### Final Connection

- Auto-reconnect
- Device discovery
- Network interface: Automatic / explicit interface

Expandable technical information:

- local IP
- mDNS service
- transport: QUIC
- TLS: 1.3
- current peer
- reconnect state

### Final Appearance

Only:

- System
- Light
- Dark

Do not add gratuitous theming controls.

### Final Advanced

- Export diagnostic logs
- Open data folder
- Reset settings
- Clear local data
- Unpair all devices
- verbose connection logging

### Final About

- application version
- check for updates
- open-source licenses
- privacy
- GitHub
- report issue

Application version must come from the actual Tauri application/package version,
not the Tauri framework version constant.

---

## 15. Android UX Model

### Android mental model

Android Cosync is a **thin trust controller and clipboard surface**.

It is not a duplicate desktop app.

The final app contains only two persistent pages:

```text
Home | Control
```

Most operational actions happen in Android system surfaces.

### Android system surfaces are part of the product

Cosync must integrate with:

- Android share sheet;
- Quick Settings;
- notification shade;
- system file picker;
- notification-access settings;
- battery/background settings.

These are first-class UX surfaces, not secondary implementation details.

---

## 16. Android Visual Language

- background: approximately `#0f1117`
- surface: approximately `#1a1c23`
- section/card: approximately `#21242d`
- accent blue: same `#3b82f6`
- same green/amber/gray state dots
- Android/system sans-serif
- native switches and Android interaction conventions
- no excessive custom animation
- system theme inheritance is preferred

No:

- 4–5 tab app navigation;
- dedicated Files page;
- notification feed page;
- large dashboard;
- internal file browser;
- duplicated transfer dashboard;
- onboarding carousel.

---

## 17. Android First Launch

Flow:

```text
Open app
→ Connect your PC
→ Scan QR
→ camera permission at the moment it is required
→ QR scanner
→ securing connection
→ Connected Home
```

### Connect screen

- monitor icon;
- "Connect your PC";
- "Open Cosync on your computer and show the pairing QR.";
- one `Scan QR` action;
- no carousel;
- no skip button;
- no dots.

### QR scanner

- full-screen camera;
- Cancel top-left;
- simple corner-bracket viewfinder;
- "Point at the QR on DESKTOP-ATELIER".

### Connecting

- compact centered content;
- device names;
- "Securing connection";
- no unnecessary spinner if the transition is short.

---

## 18. Android Home

### Initial state

```text
[ App bar: Cosync + overflow ⋮ ]

[ Large connection region ~45–50% of initial viewport ]
DESKTOP-ATELIER
● Connected
Wi-Fi · Local network

[ Clipboard ]
[ Search clipboard ]
[ History/current items ]

[ Home | Control ]
```

Important:

The trust toggles **do not live on Home**.

They live only on Control.

### Scrolled state

When clipboard history scrolls:

```text
DESKTOP-ATELIER  ● Connected · Wi-Fi · Local network
```

The connection region collapses to about 20–25% of the viewport.

Clipboard gets about 75–80%.

Bottom navigation remains visible.

### Clipboard item interaction

Once history exists:

- type icon;
- preview;
- source;
- time;
- overflow.

Tap/overflow opens a native bottom sheet:

```text
Copy
Send to PC
Delete
```

The Android app must not pretend it can globally read arbitrary clipboard content
from every foreground app when Android does not permit that.

---

## 19. Android Control

Target:

```text
[ Compact connection card ]

ALLOW THIS PC TO ACCESS

Clipboard           [toggle]
Files               [toggle]
Notifications       [toggle]

SYSTEM STATUS

Background access       Running normally >
Battery optimization    Allowed          >
Notification access     Allowed          >

Received files location Downloads/Cosync >
Connection details                       >
Disconnect                               >
Forget this PC                            >
About Cosync                              >

Your data stays between your paired devices.
```

Rules:

- toggles are trust grants;
- a toggle appears only when the capability exists;
- if enabling a feature requires a special Android permission, explain first,
  then redirect;
- Forget is destructive/red;
- Control is the canonical mobile trust surface.

### Overflow menu

Final menu:

- Connection details
- Received files
- Pause connection
- Disconnect
- Forget this PC
- About Cosync

Keep it small.

### Connection details bottom sheet

```text
Connection details

Paired device
DESKTOP-ATELIER

Connection
● Connected

Network
Wi-Fi · Local network

Latency
14 ms

Last sync
Just now

Trusted device
Yes

[ Copy device ID ]     Close
```

---

## 20. Android Permission Explanations

Use a modal/bottom sheet or other contextual overlay.

Do **not** create an entire third navigation stack of permission pages.

Example:

```text
Notification access

Cosync needs Android notification access to mirror selected
phone notifications to DESKTOP-ATELIER.

Notification content stays between your paired devices.

Cancel                         Open Settings
```

Never silently dump the user into Android Settings.

---

## 21. Android System Integrations

### Share sheet

Register Cosync for appropriate text/file share intents.

User should see a clear target such as:

```text
Send to DESKTOP-ATELIER
```

Support at least:

- `ACTION_SEND`
- `ACTION_SEND_MULTIPLE` when file transfer iteration reaches multi-file support
- shared text
- content URIs

### Quick Settings tile

State:

```text
Cosync
Connected
```

Tap:

- pause/resume device connection.

Long-press / preferences intent:

- open Cosync Control page.

Register the appropriate Quick Settings preferences activity intent so long-press
can reach Cosync rather than relying only on default App Info behavior.

### Clipboard receive notification

PC → Android:

```text
Cosync

Copied from DESKTOP-ATELIER

<preview>

[ Copy / Paste ]
```

The visible confirmation is an intentional trust UX even if Android technically
allows the write path in a specific OS version.

### Transfer progress

```text
Receiving vacation.mp4 from DESKTOP-ATELIER

██████████████░░░░ 78%

612 MB / 783 MB

Pause   Cancel
```

### Transfer complete

```text
vacation.mp4 received

Open
Show in Files
```

---

# PART II — PLATFORM AND PERFORMANCE CONTRACT

---

## 22. Desktop Platform Contract

Supported desktop platforms:

- Windows 10/11
- Linux

Both are first-class.

### Cross-platform default

Prefer:

- shared Rust core;
- Tauri for desktop windowing;
- Tauri tray API;
- Tauri global shortcut plugin;
- cross-platform storage abstraction;
- cross-platform notification API for basic notifications.

### Native divergence rule

A native implementation may be introduced only when one of these is true:

1. the cross-platform abstraction cannot implement the required UX correctly;
2. the native implementation measurably reduces latency/resource use;
3. the native implementation is necessary for platform reliability.

Document the reason and measurements.

Do not write separate Windows and Linux versions of the whole feature.

---

## 23. Clipboard Platform Reality

### Windows

Use event-driven clipboard notification:

```text
AddClipboardFormatListener
```

No idle polling.

### Linux

Linux clipboard behavior differs across X11 and Wayland.

Implementation rule:

1. prefer an event-driven watcher when the active display stack exposes a reliable
   mechanism;
2. isolate Linux clipboard watching behind a platform adapter;
3. if an event-driven method is not reliable/available, use adaptive polling;
4. polling fallback should normally be within roughly 100–250ms;
5. measure both CPU cost and P95 detection latency;
6. document whether the tested environment is X11 or Wayland.

Do not claim a 250ms polling implementation can guarantee `<200ms` end-to-end latency.

---

## 24. Android Clipboard Reality

Modern Android restricts background clipboard reads.

A foreground service is **not** permission to freely read the clipboard contents
of whatever foreground application the user is using.

Therefore the first production-safe clipboard contract is:

### PC → Android

- desktop sends clipboard content;
- Android receives it over the trusted session;
- Android shows a visible Cosync notification;
- user action copies/applies the content.

### Android → PC

Reliable MVP paths:

- Android share sheet: user shares selected text to `Send to DESKTOP-ATELIER`;
- when Cosync itself is foregrounded and Android allows clipboard access,
  Cosync may offer explicit send/copy behavior.

Do not promise silent global Android→PC clipboard monitoring in v1 unless a future
Android-compatible architecture is proven, policy-safe, and manually approved.

Do not use AccessibilityService as a clipboard workaround without a separate
architecture/policy decision.

### Future automatic Android clipboard exploration

If automatic background Android→PC clipboard capture is later pursued, treat it as a
separate experiment requiring:

- feasibility validation on current Android;
- Play Store policy review;
- battery/resource measurement;
- explicit human approval;
- a new ADR.

---

## 25. Android Foreground Service Contract

Cosync's persistent device session should use an Android foreground service whose
declared role matches a connected external device.

For modern target SDKs, configure the required foreground-service permissions,
including the service-specific connected-device permission where required.

Expected manifest/runtime responsibilities include:

- `FOREGROUND_SERVICE`
- connected-device foreground-service permission for target SDKs that require it
- `INTERNET`
- `ACCESS_NETWORK_STATE`
- `CHANGE_WIFI_MULTICAST_STATE`
- `CAMERA`
- `POST_NOTIFICATIONS` on Android versions that require runtime notification permission

Use a `connectedDevice`-appropriate foreground service type rather than treating
the always-on Cosync session as an indefinite `dataSync` service.

### mDNS on Android

Before Android mDNS discovery starts:

- acquire a `WifiManager.MulticastLock`;
- keep it for the discovery session lifetime;
- release it when discovery stops.

Do not omit this.

---

## 26. Performance Contract

Performance is a release feature.

A correct-but-slow feature is not done.

### Global rules

- no blocking work on the React render thread;
- no blocking disk/network operation on UI threads;
- no high-frequency polling where events exist;
- no unnecessary copies of large buffers;
- no unbounded queues;
- no redundant serialization;
- no unnecessary React re-renders;
- profile before blanket memoization;
- measure before introducing platform-specific complexity.

### Clipboard latency

Windows event-driven path:

```text
Target: P95 < 200ms
Measurement:
source clipboard event
→ encoded/sent
→ destination apply/visible action
```

Linux:

```text
Event-driven implementation:
target P95 < 250ms

Polling fallback:
document poll interval
target P95 < 400ms
```

Android share-to-PC:

```text
Target: < 300ms from user share confirmation to PC clipboard apply on LAN
```

PC-to-Android:

```text
Target: notification posted < 500ms P95 after desktop copy event
```

### File transfer throughput

Benchmark against `iperf3` or equivalent between the same two devices on the same
network immediately before/around the transfer test.

Target:

```text
sustained Cosync payload throughput >= 80% of measured network baseline
```

Do not compare against the theoretical Wi-Fi link rate.

### Desktop memory

At the first runnable desktop checkpoint, record:

- blank/current Tauri process baseline;
- aggregate RSS of Cosync-owned desktop processes after idle settles.

Targets:

- stretch target: aggregate idle RSS < 50MB;
- if the platform WebView baseline makes that impossible, record the baseline and
  keep Cosync's incremental overhead as low as practical;
- no accepted milestone may add >10MB steady idle RSS without measured justification.

### Android battery

Measure baseline-adjusted foreground-service idle drain over at least 4 hours.

Target:

```text
< 1% battery per hour above device idle baseline
```

Investigate any result above:

```text
2% per hour
```

A previous 5%/hour allowance is superseded.

### UI

- 60fps target on supported hardware;
- no I/O on render thread;
- connection-card collapse and clipboard scrolling must remain smooth;
- large transfer updates should be throttled/coalesced before reaching React UI.

---

## 27. Privacy and Security Contract

### Device content

Clipboard payloads, files and notification content are device-to-device data.

They must not be sent to a Cosync cloud relay.

### Logs

Never log:

- clipboard contents;
- notification bodies;
- transferred file bytes;
- private share-sheet text;
- pairing tokens.

Logs may include safe metadata such as:

- transfer ID;
- byte counts;
- state transitions;
- error codes;
- device IDs in a redacted form;
- timing measurements.

### Crash reporting

Default v1 policy:

**local diagnostic logs only**.

The user can export diagnostics manually.

Do not enable third-party crash reporting by default while simultaneously promising
that no personal/diagnostic data leaves the device.

If remote crash reporting is later introduced:

- it must be explicitly opt-in;
- payloads must be scrubbed;
- the privacy copy must distinguish device content from opt-in diagnostics;
- create/update an ADR first.

### Security

- pinned TLS certificate trust remains mandatory;
- never silently accept an unknown certificate;
- pairing token is one-time;
- paired-device store persists fingerprint + device identity information;
- forgetting a device removes trust so re-pairing is required.

---

# PART III — AGILE BUILD PLAN

---

## 28. Milestone 0 — Repository Foundations

**Status:** COMPLETE.

Preserved context:

- Cargo workspace exists.
- `cosync-core` exists.
- Tauri desktop shell exists.
- React Native / Expo bare Android project exists.
- Android NDK targets/tooling established.
- architecture decision files exist.

Do not redo this milestone.

---

## 29. Milestone 1 — Protocol & Data Model

**Status:** COMPLETE.

Preserved context:

- Protobuf protocol exists.
- `Envelope` exists.
- HLC exists.
- loop prevention exists.
- `DeviceIdentity` exists.
- `PairingPayload` exists.
- certificate identity/pinning model exists.

Do not redo this milestone unless a later vertical slice reveals a protocol defect.

---

## 30. Milestone 2 — Discovery & Secure Pairing Core

**Status:** COMPLETE.

Preserved context:

- `_cosync._udp.local` discovery exists;
- QUIC/rustls session exists;
- self-signed cert fingerprints are pinned;
- paired-device persistence exists;
- reconnect core exists;
- desktop backend includes pairing/session plumbing.

Android must still correctly acquire a multicast lock when its discovery adapter is
implemented.

---

# NEXT WORK STARTS HERE

---

## 31. Milestone 3A — Desktop Tray + Real Pairing

### Goal

Produce the smallest real desktop UI that lets the developer pair a phone and observe
connection state.

Nothing else.

No Settings window.
No clipboard-history window.
No File Transfers window.
No future UI stubs.

### Prerequisites

- Milestones 0–2 complete.
- current desktop Rust backend available.

### Iteration 1 — Functional

1. Replace the default Tauri React template with minimal real routing/surface selection.
2. Implement `PairingWindow`.
3. `PairingWindow` calls the real `get_pairing_qr`.
4. Render a real scannable QR.
5. Add real hostname command and display hostname.
6. Tray contains only what is currently real:

   ```text
   Show pairing QR
   Paired device / connection state
   Quit
   ```

7. Connection state updates from backend events; do not poll every 2 seconds if
   an event already exists.
8. A simple device row/list is allowed only as a test surface if needed to verify
   backend data.

### Automated checks

- desktop TypeScript build;
- Tauri build/dev launch;
- Rust tests;
- no console/runtime crash.

### Manual verification checklist

Developer must verify on Windows:

- tray appears;
- Show pairing QR opens;
- QR is scannable;
- real hostname is shown;
- phone can pair;
- connection state changes correctly;
- closing/reopening does not break the backend.

Developer must verify the same behavior on Linux before the iteration is accepted.

### Human gate

Agent state:

```text
READY_FOR_HUMAN_TEST — Milestone 3A / Iteration 1
```

STOP.

### Iteration 2 — Designed

Only after explicit approval.

Apply the final pairing/tray design:

- graphite surface;
- native title bar;
- correct typography;
- compact QR layout;
- LAN row;
- encryption note;
- native tray menu labels;
- no dashboard.

Do not begin another desktop window.

### Human gate

STOP after build and manual checklist.

### Context handoff

A real, styled tray + pairing window exists on Windows/Linux and uses real backend data.

---

## 32. Milestone 4A — Android Pairing + Persistent Connection

### Goal

Create the smallest Android client that:

- runs the Rust core;
- scans the desktop QR;
- pairs;
- maintains/reconnects the trusted session;
- shows connection state.

Do not build clipboard history, Files UI, notifications mirroring, or full Control
settings yet.

### Iteration 1 — Functional

1. Cross-compile `cosync-core` for required Android ABIs.
2. Generate Kotlin bindings with UniFFI.
3. Implement thin `CosyncBridgeModule`.
4. Expose only the minimum connection API:
   - start discovery/session;
   - pair with scanned payload;
   - current connection state;
   - connection-state event emitter.
5. Implement `CosyncForegroundService`.
6. Use a connected-device-appropriate foreground service declaration.
7. Add required current Android manifest permissions.
8. Acquire/release `WifiManager.MulticastLock` around mDNS discovery.
9. Typed JS/TS native wrapper.
10. Minimal Android screens:
    - Connect/Scan button;
    - QR scanner;
    - plain Connected/Disconnected result.
11. Reopen after process/activity recreation and verify reconnect.
12. Keep RN UI minimal; no final design yet.

### Manual verification

On a physical Android device:

- foreground service starts;
- service notification exists;
- QR scan pairs with real desktop;
- pinned trust is used;
- desktop shows connected;
- Android shows connected;
- Wi-Fi off/on reconnects;
- closing the activity does not destroy the intended connection lifecycle;
- app restart reconnects;
- multicast discovery works on real Wi-Fi.

Desktop side must be checked on both Windows and Linux across this slice.

### Human gate

STOP.

### Iteration 2 — Designed

After approval, style only the surfaces proven above:

- Connect your PC
- QR scanner
- brief securing-connection state
- Home connection region
- minimal Home/Control bottom navigation shell

Do not add fake feature toggles.

### Human gate

STOP.

### Context handoff

Desktop and Android form a stable paired trusted session.

This is Release Checkpoint R0.1.

---

## 33. Milestone 5 — Core Clipboard Handoff MVP

### Goal

Ship the first genuinely useful cross-device feature while respecting modern Android
clipboard restrictions.

### Important product contract

v1 does **not** claim unrestricted silent Android background clipboard reads.

The MVP is:

```text
PC → Android:
desktop copy
→ encrypted LAN transfer
→ Android visible notification
→ user Copy/Paste action

Android → PC:
user shares selected text to "Send to DESKTOP-ATELIER"
→ encrypted LAN transfer
→ PC clipboard updated
```

When Cosync itself is in the foreground and Android permits clipboard access, an
explicit in-app send action may be added, but it is not the core dependency.

### Iteration 1 — Functional

#### Protocol/core

1. Use existing clipboard envelope/data type.
2. Tag updates with HLC/source device.
3. Apply loop-prevention / causal ordering.
4. Keep payload cap at 5MB.
5. Deduplicate rapid duplicate updates with a small bounded recent-update cache.

#### Desktop → Android

6. Desktop clipboard watcher:
   - Windows: event-driven;
   - Linux: platform adapter with event-driven preferred, measured fallback polling.
7. Send text clipboard update over current QUIC session.
8. Android receives update in foreground service.
9. Post a notification with preview and explicit user action.
10. User action writes/copies the received value using Android clipboard APIs.

#### Android → Desktop

11. Register Android text share target.
12. Share sheet target label should identify the PC where possible:
    `Send to DESKTOP-ATELIER`.
13. Shared text enters Rust/core transport.
14. Desktop receives it and writes it to the PC clipboard.
15. Do not silently read arbitrary background Android clipboard content.

### Acceptance metrics

- Windows desktop copy → Android notification P95 < 500ms on LAN.
- Android share confirmation → desktop clipboard update < 300ms target.
- No ping-pong loops.
- 100 rapid alternating tests do not create runaway duplication.
- memory remains stable.

### Manual verification

Developer must test:

- Notepad/text editor → Android;
- browser selected text → Android share sheet → Windows;
- same Android share → Linux;
- Wi-Fi reconnect;
- repeated copy;
- 5MB rejection behavior;
- notification action actually places the received text into Android clipboard.

### Human gate

STOP.

### Iteration 2 — Designed

After approval:

Desktop:

- no custom incoming clipboard window;
- small native notification only if useful.

Android:

```text
Cosync
Copied from DESKTOP-ATELIER

<preview>

[ Paste / Copy ]
```

Control:

- add the real Clipboard trust toggle only now;
- if disabled, clipboard handoff is stopped.

Home:

- may show the current recent cross-device clipboard item;
- do not pretend persistent history exists yet.

Settings:

- add real desktop Clipboard controls only now.

### Human gate

STOP.

### Context handoff

Release Checkpoint R0.2 exists: pairing + reliable clipboard handoff.

---

## 34. Milestone 5B — Persistent Clipboard History + Search

### Goal

Turn the proven clipboard flow into persistent searchable history without changing
the core transport.

### Iteration 1 — Functional

1. Add `clipboard_history` table:

   ```text
   id
   content
   content_type
   source_device_id
   hlc_time
   created_at
   ```

2. Default cap: 100 items.
3. Oldest-first eviction.
4. Persist accepted cross-device clipboard events.
5. Add local items only when the platform legally/technically exposes them.
6. Expose:
   - `get_history(query)`
   - `restore_history_item(id)`
   - `delete_history_item(id)`
   - `clear_history()`
7. Restore must re-tag with current HLC and send as a fresh event.
8. Implement desktop global shortcut `Ctrl+Shift+V`.
9. Build real clipboard palette using real history.
10. Mobile Home now uses real history data.

### Manual verification

- survives restart;
- search finds expected items;
- restore creates fresh timestamp;
- delete works;
- cap eviction works;
- desktop keyboard navigation works;
- Android Home collapse remains smooth with a long history list.

### Human gate

STOP.

### Iteration 2 — Designed

Desktop palette:

- exact compact command-palette design;
- search focus;
- icons;
- metadata;
- selected row;
- footer state.

Android Home:

- large connection state initially;
- collapses on scroll;
- clipboard list gets ~80%;
- bottom sheet actions:
  `Copy | Send to PC | Delete`.

Desktop Settings Clipboard panel becomes fully real.

### Iteration 3 — Later polish

- fuzzy matching (`fuzzy-matcher`, `nucleo`, or measured equivalent);
- URL detection;
- code detection;
- image clipboard support only after text path is stable;
- accessibility;
- shortcut customization.

---

## 35. Milestone 6 — File Transfer Vertical Slice

### Goal

Send files in both directions with checksum verification and resume.

### Iteration 1 — Functional

#### Protocol

1. Send `FileMeta` first:
   - transfer ID
   - name
   - size
   - SHA-256
   - content type if known
   - resume offset/state
2. Use a dedicated QUIC stream per transfer.
3. Prefer streaming raw file bytes on the dedicated stream after metadata rather than
   wrapping every payload block in a high-overhead general-purpose envelope.
4. If the existing `FileChunk` protocol is already deeply wired, it may be used for
   the first test slice, but benchmark it and replace it if it materially misses
   the throughput target.
5. Initial I/O buffer: around 256KiB.
6. Treat buffer/chunk size as tunable, not a wire-protocol constant.
7. Benchmark roughly 64KiB / 256KiB / 1MiB before final polish.

#### Desktop

8. Send:
   - tray `Send file...` → native picker;
   - drag/drop onto File Transfers utility if supported.
9. Stream from disk; never load whole file.
10. Receive to temp path.
11. Verify SHA-256.
12. Move atomically into `Downloads/Cosync`.

#### Android

13. Register file share target.
14. Read content URI as a stream.
15. Avoid copying into JS memory.
16. Bridge data efficiently into Rust/native layer.
17. Receive to a fixed v1 location such as `Downloads/Cosync` / MediaStore-visible
    destination.
18. **Do not implement arbitrary folder picking in Iteration 1.**

#### Resume

19. Persist last acknowledged offset/chunk.
20. Wi-Fi interruption keeps partial temp file.
21. reconnect resumes rather than restarts.

### Acceptance criteria

- 100MB file both directions;
- checksum matches;
- Android file appears in Files/Gallery where appropriate;
- desktop file appears in `Downloads/Cosync`;
- Wi-Fi interruption resumes;
- clipboard traffic continues during a transfer.

### Performance acceptance

Run `iperf3` or equivalent between same devices.

Target sustained Cosync payload throughput:

```text
>= 80% of measured LAN baseline
```

### Human gate

STOP.

### Iteration 2 — Designed

Only after real transfer works:

Desktop File Transfers utility becomes real:

- Send file
- drop target
- progress
- bytes
- throughput
- ETA
- pause/cancel where supported
- completion
- open folder

Tray:

```text
Send file...
Transfers (2)
Open received folder
```

No custom progress bar inside native tray menu.

Desktop completion notification:

```text
photo.jpg received from Pixel 8
Open
Show in folder
```

Android:

- share sheet target;
- progress notification;
- completion notification;
- Control adds real Files toggle;
- desktop Settings adds real Files panel.

### Human gate

STOP.

### Iteration 3 — Later polish

- multiple-file queue;
- `ACTION_SEND_MULTIPLE`;
- folder transfer;
- adaptive buffer sizing if benchmark justifies;
- configurable receive destination using SAF persisted tree URI;
- conflict policy;
- transfer history if real user need appears.

---

## 36. Milestone 7 — Desktop Settings Foundation + Device Details

### Goal

Consolidate only the controls that now correspond to working capabilities.

This milestone may be moved earlier if the developer explicitly needs Settings for
testing, but it must not become a container full of future-feature stubs.

### Iteration 1 — Functional

Build Settings window with currently real panels only.

At minimum after clipboard/file slices:

- General
- Clipboard
- Files
- Connection
- Appearance
- About

Add Advanced only if its actions are real.

Notifications panel appears only after notification mirroring exists.

General:

- Start at login
- Keep in tray
- reconnect automatically
- connection notifications
- real paired-device list
- Pair another device

Device row opens Device Details.

Device Details:

- status
- network
- latency
- last sync
- real feature toggles only
- trust info
- open received folder
- disconnect
- forget
- copy device ID/fingerprint

### Technical corrections

- application version comes from application package info / Tauri app version;
- hostname is obtained through an explicit command;
- no device-model photo lookup;
- no remote assets.

### Human gate

STOP.

### Iteration 2 — Designed

Apply the locked settings design:

- approximately 200px sidebar;
- compact native rows;
- subdued panel styling;
- blue selected accent;
- red destructive rows;
- system theme support;
- no dashboard cards.

When all product features exist, the final sidebar is exactly:

```text
General
Clipboard
Files
Notifications
Connection
Appearance
Advanced
About
```

---

## 37. Milestone 8 — Android System Surface Completion

### Goal

Finish the Android-native surfaces that make Cosync feel integrated rather than app-centric.

Build only system integrations backed by already working features.

### Iteration 1 — Functional

1. Quick Settings tile:
   - Connected / Disconnected;
   - tap pauses/resumes;
   - preferences/long-press route opens Control.
2. Share targets:
   - text;
   - files;
   - multiple files when supported.
3. Control page:
   - real Clipboard toggle;
   - real Files toggle;
   - real background/service status;
   - battery optimization status;
   - received files location status;
   - connection details;
   - disconnect;
   - forget;
   - about.
4. Permission explanation bottom sheets before Settings redirects.
5. Fix any foreground-service notification/channel issues on modern Android.

### Human gate

STOP.

### Iteration 2 — Designed

Match the two-page product design.

No third persistent page.

---

## 38. Milestone 9 — Notification Mirroring

### Goal

Mirror selected Android notifications to Windows/Linux with a secure, low-latency path.

Quick reply is deliberately separated from basic mirroring.

### Prerequisites

- stable Android foreground/session lifecycle;
- manually approved clipboard/file slices preferred;
- Control page available for trust/permission.

### Iteration 1 — Functional mirroring only

1. Android `NotificationListenerService`.
2. Notifications trust toggle:
   - explanation modal;
   - redirect to notification-access settings.
3. Extract:
   - `StatusBarNotification.key`
   - package name
   - app label
   - title
   - body
   - post time
   - reply capability
4. Use `StatusBarNotification.key` as the primary notification identity.
   Do not rely only on an integer notification ID.
5. Add protocol message `NotificationEvent`.
6. Desktop basic native OS notification:
   - Tauri/cross-platform notification path where sufficient.
7. Default app allow-list is conservative; user opts apps in.
8. Do not send notification content anywhere except the paired device.

### App icons

Iteration 1:

- use generic application/notification icon.

Do not fetch app icons from the internet.

Later:

- Android may downscale/source the originating app icon;
- desktop may cache it by package/version;
- only if bandwidth/memory cost is negligible.

### Acceptance criteria

- WhatsApp/Telegram/etc. notification appears on Windows within ~1 second;
- same on Linux;
- blocked apps do not mirror;
- duplicate notifications are deduplicated/updated appropriately;
- dismissing/updated notification state does not crash.

### Human gate

STOP.

### Iteration 2 — Designed

- originating app name + phone source;
- compact native notification;
- no large Cosync branding;
- desktop Settings gains real Notifications panel;
- Android Control gains real Notifications toggle if not already present.

### Human gate

STOP.

---

## 39. Milestone 9B — Notification Replies

### Goal

Reply from the desktop only where the source Android notification exposes a valid
`RemoteInput` reply action.

### Important desktop platform rule

Do not assume one cross-platform Tauri notification API provides inline text reply
on both Windows and Linux.

Use a capability abstraction.

### Windows path

Preferred:

- native WinRT toast reply/action support where available;
- this is an allowed native divergence because it provides required OS integration.

### Linux path

Linux notification servers vary.

Preferred order:

1. use notification action support where available;
2. action opens a **tiny Cosync reply utility popover**;
3. user types reply;
4. reply is sent to Android.

A small reply popover on Linux is explicitly allowed.

Do not build a full Notifications window.

### Android reply path

1. desktop sends:
   - `notification_key`
   - reply text
2. Android listener finds still-active `StatusBarNotification` by key.
3. find valid `RemoteInput` action.
4. send through `PendingIntent`.
5. if stale/dismissed:
   - fail visibly but briefly;
   - no crash.

### Acceptance criteria

- supported WhatsApp notification can be replied to from Windows;
- supported notification can be replied to from Linux using the best supported
  native/action + tiny-popover path;
- unsupported notifications remain view-only;
- stale notification reply fails gracefully.

### Human gate

STOP.

---

## 40. Milestone 10 — Hardening / v1.0

### Goal

Turn the incrementally proven product into a daily-drivable release.

### Work

1. Reconnection soak:
   - Wi-Fi off/on;
   - router restart;
   - sleep/wake;
   - 2.4/5GHz switch;
   - Android reboot;
   - desktop reboot.
2. Multi-hour memory audit:
   - `heaptrack` / appropriate Windows tools;
   - Android Studio profiler.
3. Handle/file-descriptor audit.
4. Verify no `Arc<Mutex<_>>` or callback cycles leak.
5. Verify transfer temp files are cleaned safely.
6. Verify database migrations.
7. Autostart on desktop.
8. Tauri updater.
9. Windows signing.
10. Linux packaging.
11. Play Store submission/data-safety materials.
12. NotificationListenerService justification.
13. Permissions review.
14. Accessibility review.
15. Performance regression suite.
16. Privacy review.
17. Local diagnostic export.

### Remote diagnostics

No mandatory Sentry in v1.

If opt-in remote crash reporting is later desired, create an ADR first.

### Release acceptance

- all previously approved slices still pass;
- Windows installer works;
- Linux package works;
- Android release build works;
- multi-hour soak passes;
- memory stable;
- battery target acceptable;
- reconnection stable;
- no content leaves paired devices except explicitly opt-in diagnostics/update checks;
- user can use the product daily without keeping the app window open.

---

## 41. Milestone 11 — Deferred v2.0

Do not start before v1 is stable and used daily.

### Virtual webcam

Potential direction:

```text
CameraX
→ MediaCodec H.264
→ QUIC
→ PC decode
→ existing OBS Virtual Camera API where practical
```

Avoid writing a custom DirectShow virtual camera from scratch unless necessary.

### Virtual microphone

Potential direction:

```text
Android capture
→ Opus
→ QUIC
→ desktop audio endpoint integration
```

### SMS / Calls

High policy/permission risk.

Before implementation:

- validate Play Store approvability;
- validate default-SMS/default-dialer requirements;
- validate privacy implications;
- create dedicated ADR;
- create new milestone spec.

### Automatic Android background clipboard capture

Also deferred unless a safe, policy-compatible architecture is proven.

Do not use accessibility as a shortcut without explicit architectural approval.

---

# PART IV — TESTING AND HUMAN GATES

---

## 42. Per-Iteration Agent Report Template

Every agent completion message should contain this structure:

```text
ITERATION COMPLETE — READY_FOR_HUMAN_TEST

Milestone:
Iteration:

Implemented:
- ...

Files changed:
- ...

Automated checks:
- cargo test: PASS/FAIL
- typecheck: PASS/FAIL
- desktop build: PASS/FAIL
- Android build: PASS/FAIL
- other: ...

Performance measured:
- ...

Run commands:
1. ...
2. ...

Manual test checklist:
[ ] ...
[ ] ...
[ ] ...

Known limitations:
- ...

STOPPING HERE.
Waiting for explicit human approval before continuing.
```

The agent must not omit the final stop.

---

## 43. Cross-Platform Acceptance Checklist

For any desktop-affecting iteration:

- [ ] Tested on Windows 10/11
- [ ] Tested on Linux
- [ ] Behavior is equivalent
- [ ] Native divergence, if any, is documented
- [ ] system tray works on both
- [ ] notifications used by this slice work on both
- [ ] no accidental Windows-only paths
- [ ] no accidental Linux-only paths

If current CI/agent environment cannot run one platform:

- do not claim the criterion passed;
- report it as `PENDING HUMAN TEST`;
- provide exact build/run instructions;
- stop.

---

## 44. Correctness Checklist

- [ ] Real backend/data, not mock data, unless this iteration explicitly permits a shell
- [ ] Real paired device tested
- [ ] No regression in prior approved slice
- [ ] Visible error state
- [ ] No silent failure
- [ ] No hardcoded device name that should be dynamic
- [ ] reconnect tested
- [ ] destructive actions require deliberate user action
- [ ] database/state survives restart where intended

---

## 45. Performance Checklist

- [ ] No render-thread blocking
- [ ] No unnecessary polling
- [ ] Polling fallback measured
- [ ] No unbounded channels/queues
- [ ] No unnecessary large buffer copies
- [ ] Desktop idle RSS measured
- [ ] Android idle battery impact measured at release checkpoints
- [ ] Clipboard latency measured for applicable path
- [ ] File transfer throughput measured against network baseline
- [ ] Transfer UI update frequency is throttled/coalesced
- [ ] React renders profiled if a hot UI path looks suspicious

Do not add `React.memo`, `useMemo`, `useCallback` everywhere by default.
Use them where profiling shows value.

---

## 46. Design Checklist

Desktop:

- [ ] no dashboard
- [ ] tray-first
- [ ] native desktop proportions
- [ ] small utility windows
- [ ] system-like font
- [ ] no decorative gradient
- [ ] no oversized CTA
- [ ] green status only as dot
- [ ] red only destructive
- [ ] keyboard navigation where applicable
- [ ] Settings final model has 8 sections
- [ ] no separate Devices tab
- [ ] generic device icon only

Android:

- [ ] only Home + Control persistent pages
- [ ] trust toggles only on Control
- [ ] Home connection region collapses on clipboard scroll
- [ ] system share sheet is used for sending
- [ ] Quick Settings tile exists when scheduled
- [ ] transfer state uses notifications
- [ ] permissions explained before redirect
- [ ] no dedicated Files/Notifications dashboard page

---

## 47. Privacy & Security Checklist

- [ ] clipboard content never enters logs
- [ ] notification body never enters logs
- [ ] file content never enters logs
- [ ] pairing token never enters logs
- [ ] unknown cert rejected
- [ ] forgotten device must re-pair
- [ ] data transport remains within paired LAN session
- [ ] no hidden telemetry
- [ ] no internet icon/device lookup
- [ ] exported logs are safe to share

---

# PART V — IMPLEMENTATION NOTES

---

## 48. State and Event Design

Prefer event-driven updates.

Desktop React should subscribe to narrow Tauri events rather than repeatedly polling
for connection state.

Examples:

- device-connected
- device-disconnected
- reconnecting
- clipboard-received
- transfer-progress
- transfer-completed

Coalesce high-frequency progress events before sending them into React.

Do not rebuild entire device lists for every tiny transfer tick.

---

## 49. Storage

SQLite remains the local persistence layer.

Expected tables over time:

```text
paired_devices
settings
clipboard_history
transfers / transfer_resume_state (if needed)
notification_preferences
```

Add migrations explicitly.

Do not create every future table in advance unless it is needed by the current slice.

---

## 50. Network Session

A `Session` represents a live authenticated peer connection.

Core responsibilities:

- authenticated QUIC connection;
- control/envelope send;
- receive dispatch;
- heartbeat;
- reconnect;
- trusted certificate verification.

QUIC multiplexing means:

- clipboard/control does not wait behind file transfer;
- each transfer can have a dedicated stream;
- heartbeats/control remain responsive.

---

## 51. HLC / Loop Prevention

Every sync-like update must preserve:

- source device identity;
- causal ordering;
- loop prevention.

Rule:

```text
if source_device_id == self.device_id:
    drop
```

Received/restored history items must use current logical time when re-sent.

Do not rebroadcast stale history timestamps.

---

## 52. File Safety

Receiver:

```text
incoming stream
→ temporary file
→ checksum
→ atomic move / MediaStore finalization
```

Never write directly over a final destination before verification.

Interrupted transfer temp files should be resumable or safely cleanable.

---

## 53. Notification Identity

Use Android's stable `StatusBarNotification.key` for mirrored notification identity.

Do not rely only on numeric notification ID.

Protocol may include:

```text
notification_key
package_name
app_label
title
body
post_time
has_reply
```

Reply must reference the stable key.

---

## 54. App Icon Handling for Mirrored Notifications

MVP:

- generic app/notification icon.

Later:

- Android extracts/downscales source app icon;
- transfer once when needed;
- cache on desktop by package/version;
- no internet lookup.

This is optional polish, not a blocker.

---

## 55. Received Files Location on Android

Iteration 1 file transfer:

```text
Downloads/Cosync
```

or another fixed MediaStore-visible location.

Do not expose an arbitrary folder selector before the SAF persistence path is actually
implemented.

Later configurable destination requires:

- SAF tree picker;
- persisted URI permission;
- restore after reboot;
- fallback if permission revoked.

---

## 56. Desktop Notification Strategy

Basic notifications:

- use the cross-platform Tauri notification route where adequate.

Interactive replies:

- platform capability abstraction;
- WinRT/native path on Windows when required;
- Linux action support where available;
- tiny reply utility popover fallback on Linux.

No full Cosync notification-center window.

---

## 57. Dependency and Toolchain Discipline

- current stable Rust via `rustup`;
- lockfile committed;
- dependency upgrades isolated from feature work where practical;
- do not combine a large dependency upgrade with a feature iteration unless necessary;
- measure new native/plugin dependencies for binary size/idle cost where significant.

---

## 58. Final Product Definition of Done

Cosync v1 is complete when a user can:

1. install desktop on Windows or Linux;
2. install Android app;
3. pair locally via QR;
4. reconnect automatically;
5. send clipboard content from PC to Android with visible trust confirmation;
6. send selected text from Android to PC through Android's native share surface;
7. search/restore cross-device clipboard history;
8. send files in both directions;
9. resume interrupted file transfers;
10. mirror allowed Android notifications to desktop;
11. reply to supported notifications using the best native platform path;
12. control access from Android Control;
13. manage device/settings from compact desktop Settings;
14. leave both applications running quietly in the background;
15. use the system tray/share sheet/notification shade for normal daily actions.

And the product must:

- remain local-first;
- remain low-latency;
- remain low-resource;
- work on Windows and Linux;
- avoid dashboards;
- avoid unnecessary app surfaces;
- survive real sleep/reconnect/reboot behavior;
- preserve pinned device trust;
- never require a cloud relay for core functionality.

---

## 59. Immediate Next Instruction for an Agent

Unless the developer explicitly says otherwise, the next task after adopting this SPEC is:

```text
Implement Milestone 3A — Desktop Tray + Real Pairing,
Iteration 1 ONLY.

Do not build Settings.
Do not build Clipboard History.
Do not build File Transfers.
Do not build Android.
Do not polish the UI beyond what is needed for manual testing.

When it builds:
- report the changes,
- provide run commands,
- provide the manual Windows/Linux test checklist,
- mark READY_FOR_HUMAN_TEST,
- STOP.

Wait for explicit approval.
```

---

## 60. Final Rule

When uncertain whether to build more:

> **Build less, make it real, make it runnable, measure it, let the human test it, then stop.**

That is the Cosync development methodology.
