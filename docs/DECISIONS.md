# Architecture Decision Records

## ADR-001: React Native (Expo) over Flutter for the mobile client

**Status:** Accepted

**Decision:** The mobile client is React Native, built on Expo's bare
workflow (via `expo prebuild`), not Flutter.

**Reasoning:**
- No prior Flutter/Dart experience; React Native/Expo is the known stack.
- The desktop shell (Tauri) already uses React/TypeScript. Sharing that
  language and paradigm across both frontends means shared conventions,
  and no separate skillset required if anyone else ever contributes.
- The hardest parts of the mobile client — foreground service,
  `NotificationListenerService`, `AccessibilityService`, `MediaCodec`,
  the `WifiManager` multicast lock — are Android-native concerns either
  way. Neither Flutter nor React Native abstracts them away, so the
  framework choice doesn't touch the actual hard part of this project.

**Trade-off accepted:** `flutter_rust_bridge` (Flutter's Rust FFI story)
is more ergonomic than the React Native equivalent. This project instead
uses `uniffi-rs` to generate Kotlin bindings from `cosync-core`, wrapped
in a hand-written native module exposed to JS. More moving parts, but a
one-time cost, not an ongoing one — see Milestone 4.

---

## ADR-002: macOS is out of scope

**Status:** Accepted

**Decision:** Cosync does not target macOS, on desktop or as a build
platform, for the foreseeable roadmap.

**Reasoning:**
- LinkMyMac — the direct inspiration for this project — already covers
  Android-to-Mac well.
- The actual gap in the market is Android-to-Windows (and eventually
  Linux), which nothing currently serves with the same feature depth.
- Every hour spent on macOS-specific plumbing (CoreMediaIO DAL plugins,
  `.dmg` notarization, Apple code signing) is an hour not spent on the
  thing nobody else is doing: Windows and Linux.

**Priority order:** Android → Windows (primary), Linux (secondary), iOS
only if real demand shows up later.

---

## ADR-003: LocalSend was evaluated and not forked

**Status:** Accepted

**Decision:** Cosync's core sync engine is built from scratch, not by
forking or stripping down LocalSend (github.com/localsend/localsend),
despite LocalSend being a mature, Apache-2.0-licensed, 80k+-star
cross-platform local file-sharing app.

**Reasoning:**
- LocalSend's entire interaction model is ephemeral and consent-gated —
  open the app, discover nearby devices, pick one, the other side
  approves each transfer. It has no persistent background service and
  no silent auto-sync.
- Cosync's premise is the opposite: two specific paired devices staying
  silently, continuously in sync (clipboard, notifications, eventually
  webcam) with no per-action approval. Retrofitting that onto LocalSend
  means replacing almost everything except low-level networking
  utilities — closer to rewriting than reusing.
- LocalSend is 87.5% Dart/Flutter. Forking it would directly contradict
  ADR-001.

**What was kept from studying it:** the pinned self-signed-certificate
fingerprint model (see `localsend/protocol`) validated the pairing
design already planned for Milestone 2. Their documented Android
`saf_stream` performance issue is worth knowing about before Milestone 6
(File Transfer).

---

## ADR-004: Version pins in Cargo.lock reflect the CI/sandbox toolchain, not a project constraint

**Status:** Accepted, revisit on your own machine

**Context:** Several crates (`ed25519-dalek`, `indexmap`, `zeroize`,
`base64ct`, `tempfile`) are pinned to older-than-latest versions in
`Cargo.lock`. This is *not* a deliberate architecture choice — it's
because the sandbox this milestone was built in only had Rust 1.75
(2023) available via `apt`, and the current versions of those crates
require newer `rustc` (1.78–1.86 depending on the crate).

**Action for you:** On your own machine, install Rust via `rustup`
(not your OS package manager) so you're on a current stable toolchain,
then run `cargo update` to pick up the latest compatible versions of
everything. Nothing in the code depends on the old versions — this is
purely a toolchain artifact of where Milestone 1 happened to be built.
