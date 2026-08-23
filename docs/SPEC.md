# Cosync — Development Specification

## How to Use This Document

Each milestone below is self-contained: **Goal → Prerequisites → Implementation Steps → Acceptance Criteria → Context Handoff**. Hand a single milestone to a developer (human or AI coding agent) as their task brief. Don't move to the next milestone until the current one's Acceptance Criteria pass. The Context Handoff section at the end of each milestone is what the *next* milestone assumes exists — read it before starting the next phase, whether you're a human re-onboarding after a break or an AI agent picking up mid-project with no memory of earlier sessions.

This spec supersedes earlier drafts in two ways, based on decisions made along the way:
- **Mobile stack is React Native (Expo), not Flutter.** No Flutter experience on the team; React Native/Expo is the known quantity.
- **macOS is explicitly out of scope.** LinkMyMac already owns that lane well. Cosync's target is Android → Windows first, Linux second, iOS only if demand materializes later.
- **Virtual webcam/mic/SMS/calls (Milestone 10) are deferred by design**, not forgotten — they're the highest-risk, highest-effort features and shouldn't be started until the core product is a genuinely reliable daily driver.

---

## Architecture Summary

| Layer | Choice | Why |
|---|---|---|
| Shared core logic | Rust (`cosync-core` crate) | Write networking, crypto, sync logic once, share everywhere |
| Desktop shell | Tauri v2 + React/TypeScript | Small binaries, native Rust backend, direct Rust↔JS IPC |
| Mobile shell | React Native (Expo, **bare workflow / custom dev client**) | Matches existing RN/Expo experience |
| Rust↔Mobile bridge | `uniffi-rs` → Kotlin bindings → thin native module → RN JS API | UniFFI doesn't emit JS directly; Kotlin is the practical middle step on Android |
| Discovery | `mdns-sd` crate (mDNS/DNS-SD) | Cross-platform, no central server |
| Transport | QUIC via `quinn` + `rustls` | TLS 1.3 built in, multiplexed streams, UDP resilience |
| Pairing trust model | QR-exchanged, pinned self-signed certs (not CA-validated) | Closed pairwise trust — appropriate for a personal-device mesh |
| Payload encoding | Protobuf via `prost` | Fast, small, versionable wire format |
| Local storage | SQLite via `rusqlite` (bundled) | Works identically cross-compiled for Android via NDK |

**Critical note for the mobile team:** Expo Go (the app from the Play Store) **cannot** run this project past Milestone 0. Foreground services, `NotificationListenerService`, `AccessibilityService`, custom native modules, and JNI-linked Rust binaries all require `expo prebuild` (bare workflow) or a custom Expo Dev Client. Do not attempt to develop feature milestones inside plain Expo Go — it will silently fail to expose the native modules.

### Repo Layout

```
cosync/
├── Cargo.toml                    # workspace root
├── crates/
│   └── core/                     # cosync-core: all shared Rust logic
├── apps/
│   ├── desktop/                  # Tauri + React
│   │   └── src-tauri/
│   └── mobile/                   # React Native (Expo, prebuilt/bare)
│       └── android/
│           └── rust-bridge/      # Kotlin module wrapping UniFFI bindings
└── docs/
    ├── SPEC.md                   # this document
    └── DECISIONS.md              # architecture decision records (ADRs)
```

---

## Milestone 0 — Environment & Repo Foundations

**Goal:** Tooling installed, monorepo scaffolded, three empty-but-buildable shells exist. Nothing "smart" yet.

**Prerequisites:** None.

**Implementation Steps:**
1. Create the monorepo and Cargo workspace root; add `crates/core` as the first member (`cargo new --lib crates/core --name cosync-core`).
2. Install Rust Android targets: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
3. Install `cargo-ndk` and the Android NDK (via Android Studio's SDK Manager); set `ANDROID_NDK_HOME`.
4. Add `uniffi` and `uniffi-build` to `cosync-core`'s `Cargo.toml` (can sit behind a `mobile-bindings` feature flag to avoid pulling UniFFI into the desktop build).
5. Scaffold the desktop app: `npm create tauri-app@latest apps/desktop -- --template react-ts`.
6. Scaffold the mobile app: `npx create-expo-app apps/mobile --template blank-typescript`, then run `npx expo prebuild` inside it — this generates and commits a real `android/` folder you now own and edit directly (this is the step that takes you out of Expo Go territory).
7. Create the `apps/mobile/android/rust-bridge/` Android library module and wire it into `settings.gradle`.
8. Create `docs/DECISIONS.md`; log ADR-001 (React Native over Flutter — no Flutter experience) and ADR-002 (macOS excluded from scope — already served by LinkMyMac).
9. Verify: `cargo build` succeeds at the workspace root; `npm run tauri dev` opens a blank desktop window; `npx expo run:android` builds and installs a blank RN app on an emulator or device.

**Acceptance Criteria:** All three shells build and launch with zero shared logic. Nothing crashes. `docs/DECISIONS.md` exists with the two ADRs above.

**Context Handoff:** Three empty, buildable shells exist (`cosync-core`, Tauri desktop app, RN mobile app in bare/prebuilt form). Next milestone adds the first real logic to `cosync-core` — no networking yet, just data types.

---

## Milestone 1 — Rust Core: Protocol & Data Model Foundation

**Goal:** Define the wire vocabulary devices will use to talk to each other. Pure data/logic, no networking.

**Prerequisites:** Milestone 0.

**Implementation Steps:**
1. Add `prost` + `prost-build` to `cosync-core`. Create `proto/cosync.proto` defining an `Envelope` message (a `oneof` over `PairingRequest`, `ClipboardUpdate`, `FileMeta`, `FileChunk`, `Heartbeat`, `Ack`), with every variant carrying `device_id`, `logical_time`, and `physical_time_ms` for the Hybrid Logical Clock.
2. Write `build.rs` so `prost-build` compiles the `.proto` into Rust structs at build time.
3. Implement a `HybridLogicalClock` type in `cosync_core::hlc`: `now()`, `receive(remote_time)` (updates local clock to `max(incoming, local) + 1`), and comparison ordering. Write unit tests covering the loop-prevention rule — a device must drop any update where `source_device_id == self.device_id`.
4. Implement `DeviceIdentity`: generates an Ed25519 keypair on first run (`ed25519-dalek` or `ring`), persists it via the `directories` crate's cross-platform app-data path.
5. Define `PairingPayload` (what gets QR-encoded): device name, public key fingerprint, IP hint, port, one-time pairing token — serialized as JSON (small, human-debuggable; protobuf is reserved for the ongoing wire protocol, not the QR payload).
6. Write unit tests: round-trip serialize/deserialize every `Envelope` variant; HLC ordering correctness; keypair persists correctly across a simulated restart.

**Acceptance Criteria:** `cargo test -p cosync-core` passes. No network code exists yet — everything is pure data and logic, independently testable.

**Context Handoff:** `cosync-core` now exports `Envelope`, `HybridLogicalClock`, `DeviceIdentity`, and `PairingPayload`. These are the types the discovery/pairing milestone builds on.

---

## Milestone 2 — Discovery & Secure Pairing

**Goal:** Two devices on the same LAN find each other, exchange keys via QR, and establish a mutually authenticated QUIC tunnel. No app data flows yet — just a reliable "connected" state.

**Prerequisites:** Milestone 1.

**Implementation Steps:**
1. Add `mdns-sd`, `quinn`, and `rustls` to `cosync-core`.
2. Implement a `Discovery` module: advertise a `_cosync._udp.local` mDNS service; implement a listener surfacing discovered peers as an event stream (a `tokio::sync::mpsc` channel), consumable from both the Tauri and UniFFI/Kotlin sides.
3. **Android-specific requirement — do not skip:** before starting the mDNS listener on Android, acquire a `WifiManager.MulticastLock` and hold it for the discovery session's lifetime. Android silently drops multicast/mDNS packets without this lock; it's the single most commonly missed step in Android mDNS implementations.
4. Implement the pairing flow: desktop generates its `PairingPayload` and renders it as a QR code (`qrcode.react` on the Tauri/React side, fed by a Tauri command). Mobile scans it via `react-native-vision-camera` plus a barcode-detection plugin, called through the native module.
5. On scan, the mobile side dials the desktop's advertised IP:port over QUIC, presenting its own self-signed cert. Both sides pin the peer's cert fingerprint from the QR payload — this is a closed pairwise trust model, not CA validation; document this explicitly as an intentional design choice, not an oversight.
6. Persist paired-device records (`device_id`, cert fingerprint, last-known IP) in a `paired_devices` SQLite table.
7. Implement reconnection: on app start, scope mDNS discovery to known `device_id`s for already-paired devices; auto-reconnect QUIC when found; use exponential backoff when not found so you're not hammering the network.

**Acceptance Criteria:** On two devices on the same Wi-Fi (or a desktop + Android emulator/device), pairing via QR succeeds. Killing and restarting either app re-establishes the tunnel without re-scanning.

**Context Handoff:** `cosync-core` now exposes a `Session` type representing a live, authenticated connection to a specific paired device, plus `send(Envelope)` / `on_receive(callback)`. Nothing meaningful is sent over it yet — that starts in Milestone 5.

---

## Milestone 3 — Desktop Hub Shell (Tauri)

**Goal:** A real, minimal, usable desktop app — tray icon, pairing UI, live connection status. This is the "hub" the phone talks to for the rest of the project.

**Prerequisites:** Milestone 2.

**Implementation Steps:**
1. Wire `cosync-core` into `apps/desktop/src-tauri` as a path dependency. Expose Tauri commands: `start_discovery()`, `get_pairing_qr()`, `list_paired_devices()`, `get_connection_status(device_id)`.
2. Build a system tray (Tauri's tray API): icon reflects connected/disconnected state; menu offers "Show pairing QR," "Paired devices," "Quit."
3. Build a minimal React UI: a pairing screen (renders the QR from `get_pairing_qr`) and a device list screen (live status via an event-driven or polled Tauri→JS bridge).
4. Use Tauri's path API for the app-data directory, matching where `cosync-core` keeps its SQLite file.

**Acceptance Criteria:** Launching the app shows the tray icon. Clicking it opens the pairing QR. After pairing from a phone (Milestone 2's flow), the device list shows "Connected."

**Context Handoff:** The desktop shell is a real running app with tray + pairing UI. Every later desktop feature extends this app rather than starting fresh.

---

## Milestone 4 — Android Client Shell (React Native + Rust Bridge)

**Goal:** A real, minimal Android app wired to the same Rust core, with the mandatory foreground service in place *before* any feature work begins.

**Prerequisites:** Milestone 2 (core session logic exists).

**Implementation Steps:**
1. Cross-compile `cosync-core` for Android: `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 build --release`. Place the resulting `.so` files into `apps/mobile/android/rust-bridge/src/main/jniLibs/<abi>/`.
2. Run `uniffi-bindgen generate --language kotlin` against `cosync-core`'s interface definition; place the generated Kotlin under `rust-bridge/src/main/java/...`.
3. Write a thin Kotlin wrapper, `CosyncBridgeModule`, implementing React Native's `ReactContextBaseJavaModule` (classic bridge is fine to start; migrate to a Turbo Module/Codegen later if profiling demands it). Expose `startDiscovery()`, `getPairingPayload()`, `pairWithScannedPayload(json)`, and an event emitter for connection-status changes.
4. Register the module in `MainApplication.kt`'s package list.
5. Implement `CosyncForegroundService.kt`, hosting the Rust session/connection lifecycle. It must be started before any discovery/pairing call and shows a persistent notification ("Cosync is active"). This is non-negotiable given Android's background-kill behavior — build it now, not as an afterthought.
6. On the TypeScript side, build a typed `NativeCosync` wrapper around `NativeModules.CosyncBridgeModule`, plus a `useCosyncConnection()` hook mirroring the desktop's connection-status pattern.
7. Build minimal RN UI: a pairing screen (camera scan) and a "Connected to PC" status screen.
8. Add required manifest entries: `FOREGROUND_SERVICE`, `CHANGE_WIFI_MULTICAST_STATE`, `CAMERA`, `INTERNET`, `ACCESS_NETWORK_STATE`; declare the foreground service type (e.g. `connectedDevice` or `dataSync`) — check this against the current requirement for your target SDK version, as Android's foreground-service-type rules have tightened across recent releases.
9. On first successful pairing, prompt the user toward `Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`, with clear in-app copy explaining why first (Play Store review expects this justification to be visible to the user, not just present in code).

**Acceptance Criteria:** Fresh install → scan the desktop's QR → foreground notification appears → status screen shows "Connected." Killing the app from Recents and reopening it reconnects automatically — this validates that Milestone 2's reconnection logic survives real Android process death, which is stricter than a desktop app quit.

**Context Handoff:** Android now has a working native bridge to the same Rust core the desktop uses, a persistent foreground service, and a working pairing/reconnect UI. Every later mobile feature milestone adds Kotlin bridge methods + JS wrapper calls to this same pattern — don't rebuild the bridge per feature.

---

## Milestone 5 — Bidirectional Clipboard Sync

**Goal:** The headline feature. Copy on either device, paste correctly on the other, with no loops or corruption.

**Prerequisites:** Milestones 3 and 4 (both shells exist and can exchange `Envelope`s).

**Implementation Steps:**
1. Desktop: implement clipboard read/write via `arboard` inside `cosync-core`. **Do not poll at 20ms.** Use native change-notification where the platform provides one (Windows `AddClipboardFormatListener` via a small platform-specific module) and fall back to a modest 250ms poll only where no native hook exists (X11/Linux).
2. Wrap outgoing clipboard changes in `Envelope::ClipboardUpdate`, tagged with the device's current HLC value and `source_device_id`.
3. On receive, apply the loop-prevention and HLC-ordering rules from Milestone 1 — only apply the update locally if it's causally newer.
4. Android: implement `ClipboardManager.OnPrimaryClipChangedListener` inside the foreground service (event-driven, not polled), routed through the same `Envelope::ClipboardUpdate` path via the Kotlin bridge.
5. **Android 12+ restriction:** background clipboard *writes* originating from a PC update require visible user acknowledgment. Implement a heads-up notification with a "Paste" action rather than attempting a silent background write, which the OS blocks.
6. Enforce a protocol-level size cap (e.g., 5MB) on clipboard payloads; reject/truncate oversized items with a clear error in both UIs.
7. Add a small in-memory ring buffer of recent updates to collapse duplicate rapid-fire clipboard events from the same source (several apps fire multiple clipboard writes per user copy action).

**Acceptance Criteria:** Copying text on the phone appears in the PC clipboard within ~1 second. Copying on PC shows a paste-confirmation bubble on the phone; tapping it pastes correctly. Rapid alternating copies on both devices don't loop or ping-pong.

**Context Handoff:** Clipboard sync is live end-to-end. The `Envelope` dispatch pattern established here (HLC tag, loop prevention, size cap) is the template File Transfer and later features reuse.

---

## Milestone 6 — File Transfer

**Goal:** Drag-and-drop / share files between devices, verified and safely resumable.

**Prerequisites:** Milestone 5 (proves the Envelope + QUIC-stream pattern works for a real feature).

**Implementation Steps:**
1. Define chunking in `cosync-core`: a `FileMeta` message (filename, size, SHA-256, chunk count) sent first, followed by ordered `FileChunk` messages (64KB each) on a dedicated QUIC stream — QUIC's multiplexing keeps this from blocking clipboard/heartbeat traffic.
2. Sender side: desktop reads dropped files via Tauri's drag-drop event and Rust's `std::fs`, streaming rather than loading the whole file into memory; Android reads via `MediaStore`/Storage Access Framework, returning a content-URI stream to Rust through the Kotlin bridge.
3. Receiver side: write incoming chunks to a temp file, verify the SHA-256 against `FileMeta` once complete, then move into place — the Downloads folder on desktop, an app-specific media folder plus a `MediaStore` insert on Android so the file actually shows up in the phone's gallery/file manager.
4. Handle interruption: if the QUIC stream drops mid-transfer, keep the partial temp file and support resuming from the last acknowledged chunk on reconnect (add a simple resume-offset to the `Ack` response).
5. Desktop UI: drop-zone plus a transfer progress list. Mobile UI: incoming-transfer notification and progress screen, with "Save"/"Open" actions on completion.

**Acceptance Criteria:** A 200MB video dragged from desktop arrives intact on the phone (checksum matches) and appears in the gallery. Toggling Wi-Fi off mid-transfer and back on resumes rather than restarting from zero.

**Context Handoff:** A second full feature is proven through the same Session/Envelope pipe, including a resumability pattern later milestones can reuse if needed.

---

## Milestone 7 — Clipboard History & Search

**Goal:** Persistent, searchable clipboard history on both ends — the core value Cliped demonstrated, brought properly into Cosync.

**Prerequisites:** Milestone 5.

**Implementation Steps:**
1. Add a `clipboard_history` SQLite table (`id`, `content`, `content_type`, `source_device_id`, `hlc_time`, `created_at`) in `cosync-core`. Every accepted `ClipboardUpdate` gets appended, capped at the last 100 items with oldest-eviction.
2. Add fuzzy search over history rows using a local, offline matching crate (`fuzzy-matcher` or `nucleo`) — no need for a full search-index engine at this scale.
3. Expose `get_history(query: Option<String>) -> Vec<HistoryItem>` and `restore_history_item(id)` through both Tauri commands and the Android bridge.
4. Desktop UI: a searchable, click-to-restore history panel.
5. Mobile UI: a history screen with a search bar; tapping an item to restore it locally triggers the same paste-confirmation flow as Milestone 5 unless it's a purely local write.

**Acceptance Criteria:** History persists across app restarts on both devices. Searching surfaces the right item. Restoring an old item re-syncs it to the other device as a fresh, correctly HLC-tagged update — not a re-broadcast of a stale timestamp.

**Context Handoff:** The full Tier 1 feature set plus clipboard history is complete and shippable as a v1.0 candidate in its own right.

---

## Milestone 8 — Notification Mirroring & Quick Replies

**Goal:** Read Android notifications on desktop; reply where the source app supports it.

**Prerequisites:** Milestone 4's foreground service pattern. Independent of Milestones 5–7, but benefits from the Envelope pipe being battle-tested first.

**Implementation Steps:**
1. Request `NotificationListenerService` access — a special, user-granted Settings permission, not a manifest runtime permission. The pairing UI should deep-link to the correct Settings screen with clear in-app justification copy (Play Store review expects this).
2. Implement the listener: on `onNotificationPosted`, extract app name/icon, title, and text — and check `notification.actions` for a `RemoteInput`-capable action, since that's how you detect whether quick-reply is even possible for that notification.
3. Serialize to a new `Envelope::NotificationEvent` (title, body, package name, notification ID, `has_reply: bool`); send to desktop.
4. Desktop UI: a panel showing incoming notifications; if `has_reply`, show a reply box.
5. On reply submit, send `Envelope::NotificationReply(notification_id, text)`. Android's listener looks up the still-active `StatusBarNotification` by ID, retrieves its `RemoteInput` action, and fires it via `PendingIntent.send()` with the input bundle populated. This only works while the original notification is still active — fail gracefully with a clear error if it's already been dismissed, rather than crashing.
6. Maintain an allow/deny list by package name (desktop-side, user-editable) rather than mirroring every system notification by default.

**Acceptance Criteria:** A WhatsApp message on the phone appears on desktop within ~1 second. Typing a reply on desktop and sending it actually delivers the message from the phone.

**Context Handoff:** The full Tier 2 workflow feature set is complete — a natural v1.5 release point.

---

## Milestone 9 — Production Hardening

**Goal:** Turn a working prototype into something trustworthy enough to run unattended, always-on, in the background.

**Prerequisites:** Milestones 5–8 (or whichever subset you've shipped — hardening applies to whatever exists).

**Implementation Steps:**
1. Integrate crash reporting (Sentry or similar) in both the Tauri Rust/JS layers and the Android Kotlin/RN layers — scrub clipboard content from crash payloads; never log actual clipboard text.
2. Wire up Tauri's built-in auto-updater against your own release feed (a GitHub Releases JSON endpoint is enough at this stage).
3. Stress-test reconnection: script Wi-Fi toggling, band switching (2.4GHz/5GHz), and USB plug/unplug on a real device over several hours; fix whatever edge cases surface — this is where gaps in your own reconnect logic show up, not QUIC's.
4. Run a memory/handle-leak audit: `valgrind`/`heaptrack` on desktop, Android Studio's profiler on mobile, over a few hours of simulated use — specifically check for un-dropped `Arc<Mutex<_>>` cycles and unclosed file handles.
5. Acquire a Windows code-signing certificate (or use a free option like SignPath.io's open-source program if Cosync stays open source; budget for a paid EV cert if it stays closed).
6. Prepare Play Store submission materials: data-safety declarations and justification copy for `AccessibilityService`/`NotificationListenerService` usage — expect at least one review round-trip and budget the time for it.

**Acceptance Criteria:** The app installs with no security warnings, survives a multi-hour unattended soak test without leaking memory or losing connection state, and passes an internal pre-review checklist against Play Store's policies.

**Context Handoff:** This is the real v1.0 — the point where it's a genuine daily driver, and where you'd feel fine handing it to someone else if you ever choose to.

---

## Milestone 10 — Deferred (v2.0): Virtual Webcam, Mic, SMS/Calls

**Status:** Intentionally under-specified. Documented here for completeness, not as an active task list. Don't start this until Milestone 9 is genuinely done and you've used the app daily for a while — this is the highest-risk, highest-effort part of the entire project.

**Notes for when you're ready to scope it properly:**
- **Virtual webcam:** `CameraX` capture → `MediaCodec` H.264 encode → QUIC stream → PC-side decode (`openh264` or `ffmpeg-next`) → feed into the existing OBS Virtual Camera plugin API rather than writing a custom DirectShow filter from scratch.
- **Virtual mic:** same pipeline shape, with an Opus codec (`audiopus` crate) in place of H.264.
- **SMS/Calls:** requires either being set as the device's default SMS app or requesting `READ_SMS`/`READ_CALL_LOG`, both Play Store-restricted permission categories requiring a "core functionality" declaration and likely additional Google review — validate this is even approvable for your use case before investing engineering time.

When you're ready to actually build this milestone, it should get the same Goal/Prerequisites/Steps/Acceptance/Handoff treatment as everything above — worth a dedicated planning pass rather than an extension of this one.
