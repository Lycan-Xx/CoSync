# Cosync — Agent & Developer Reference

Read this before writing any code. These rules apply to every task,
every milestone, every iteration.

---

## Shell preference

Use Git Bash as the default shell and write Bash-compatible commands.

Prefer: `pwd`, `ls`, `find`, `grep`, `sed`, `awk`, Unix-style paths.

Use PowerShell syntax only when:
1. Git Bash cannot perform the operation, or
2. A Windows-specific command or tool requires it.

State explicitly when falling back to PowerShell.

---

## Commit standard

All commits must be detailed and explanatory, following the milestone 1
and milestone 2 commit-message format. Commit messages must clearly state
the milestone or scope, summarize the completed work, and explain the
important implementation changes and verification performed.

---

## Platform targets

Cosync runs on **Windows 10/11 and Linux**. Both platforms are
first-class. Every feature must work on both before a milestone is
marked done.

Use cross-platform APIs by default:
- UI: Tauri (webview-based, works on both)
- Notifications: `tauri-plugin-notification` (WinRT on Windows,
  libnotify on Linux)
- System tray: Tauri tray API
- Global shortcuts: `tauri-plugin-global-shortcut`
- Clipboard read/write: `arboard` crate

The **only documented exception** to cross-platform clipboard handling:
- Windows: use `AddClipboardFormatListener` (event-driven, zero idle cost)
- Linux: use a 250ms poll via `arboard` (no equivalent event API exists)

Any other platform-specific divergence must be documented in
`docs/DECISIONS.md` as an ADR with a measured justification.

---

## Performance contract

Speed, low latency, and low resource usage are the top priorities.
A correct-but-slow feature is not done.

**Targets (non-negotiable):**
- Clipboard sync latency: < 200ms end-to-end on local Wi-Fi
- File transfer throughput: within 80% of raw Wi-Fi throughput
- Desktop idle memory: < 50MB RSS
- Android foreground service battery drain: < 5% per hour (4h soak)
- UI: 60fps, no janky transitions

**Rules:**
- No polling where event-driven alternatives exist
- No blocking the main/render thread
- No unnecessary memory allocation in hot paths
- No redundant React re-renders (profile before adding memoization
  everywhere — only where it measures)
- When two approaches are equally correct, choose the faster one
- If you choose the slower path, document why in the code

---

## Repo layout

```
cosync/
├── Cargo.toml               # workspace root (core + desktop)
├── crates/core/             # cosync-core: all shared Rust logic
├── apps/
│   ├── desktop/             # Tauri v2 + React/TypeScript
│   │   └── src-tauri/       # Rust backend
│   └── mobile/              # React Native (Expo bare workflow)
│       └── android/
│           └── rust-bridge/ # Kotlin module wrapping UniFFI bindings
├── docs/
│   ├── SPEC.md              # full milestone development spec
│   └── DECISIONS.md         # architecture decision records (ADRs)
├── AGENTS.md                # this file
└── CHANGELOG.md
```

---

## Architecture decisions (summary)

Full rationale in `docs/DECISIONS.md`. Summary:

| Decision | Choice | Why |
|---|---|---|
| Mobile framework | React Native (Expo bare) | No Flutter experience; shared TS paradigm with desktop |
| macOS | Out of scope | LinkMyMac already owns it |
| Transport | QUIC (`quinn`) | TLS 1.3 built-in, multiplexed, UDP resilience |
| Pairing trust | Pinned self-signed cert fingerprints | No CA needed in a two-device local mesh |
| Cloud | None | Local LAN only, always |
| v2.0 features | Deferred | Webcam/mic/SMS — don't start until v1.0 is a daily driver |

---

## UI rules (desktop)

The full design system is in `docs/SPEC.md`. Short version:

- **Tray-first.** No home screen. Tray → feature directly.
- **Small utility windows.** Not dashboards.
- Dark charcoal (`#1a1a1f` surface), system sans-serif, blue accent
  (`#3b82f6`) for active state only, green/amber/gray dots for
  connection status only.
- Settings sidebar: **8 items — General, Clipboard, Files,
  Notifications, Connection, Appearance, Advanced, About**.
  No separate Devices tab. Device management lives in General.
- Device list items use a **generic phone icon only**. No device
  photos. No model lookups. No network requests for images.
- Notification quick replies use **native OS action buttons only**
  (WinRT toast actions on Windows, libnotify actions on Linux).
  No custom Cosync reply window.
- Red is for destructive actions only. Never use red for status.

---

## UI rules (Android)

- **Two pages: Home and Control.** Bottom nav only.
- Home page: large connection card + clipboard list. **No trust
  toggle cards on Home.** Those are on Control only.
- Control page: trust toggles (Clipboard/Files/Notifications) +
  system status + storage + connection management.
- First launch: camera opens immediately on "Scan QR". No carousel.
- System surfaces: share sheet target, Quick Settings tile, clipboard
  paste notification, transfer notifications. These matter as much
  as the app itself.
- Permission screens: always explain before redirecting to Android
  Settings. Never dump the user into Settings silently.

---

## Iteration model

Every feature goes through three passes. Never skip ahead:

```
Iteration 1 — Functional:  it works, wiring is real, UI is minimal
Iteration 2 — Designed:    matches the design system, looks right
Iteration 3 — Polished:    keyboard nav, animations, edge cases, a11y
```

Don't polish before it works. Don't make it pretty before it's correct.
Don't build the full feature in one pass.

---

## Milestone checklist

Before marking any milestone done:

- [ ] Works with a real paired device
- [ ] Tested on Windows AND Linux
- [ ] No regressions in previously-completed milestones
- [ ] UI matches the design system
- [ ] Settings sidebar has 8 items (no Devices tab)
- [ ] No polling where event-driven alternatives exist
- [ ] Clipboard sync latency < 200ms on local Wi-Fi
- [ ] Desktop idle memory not regressed
- [ ] No clipboard content in logs or error reports
- [ ] No personal data leaves the LAN
