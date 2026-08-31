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

---

## ADR-005: Certificate fingerprint pinning, not raw-Ed25519-key pinning

**Status:** Accepted (refines ADR-003's pairing model)

**Decision:** The value pinned for peer trust — embedded in the pairing
QR code, checked on every handshake — is the SHA-256 fingerprint of the
device's self-signed X.509 certificate (`cert.rs`), not a fingerprint of
the raw Ed25519 key from `identity.rs` (Milestone 1).

**Reasoning:** This is the standard "TLS certificate fingerprint
pinning" shape (the same one LocalSend uses, per ADR-003's research) and
maps directly onto rustls's `ServerCertVerifier`/`ClientCertVerifier`
traits, which operate on certificates, not raw keys. `DeviceIdentity`
(Ed25519) remains available for any future signing/verification need
that isn't the TLS layer itself; `DeviceCertificate` is a separate,
purpose-built identity for the transport.

**Real bug this caught:** while testing this, a test asserting "an
unpinned client certificate must be rejected" initially appeared to
fail — the attacker's connection looked accepted. It wasn't a security
bug: TLS 1.3's client-auth handshake lets the *dialer's* side resolve
before the server finishes validating the client cert (client
certificate is the second handshake flight), so `connect().await`
returning `Ok` doesn't by itself prove acceptance. The fix was in the
test (wait on `connection.closed()` to observe the server's actual
verdict), not in `verifier.rs`. Documented here because it's a subtlety
worth remembering before trusting any future "did the handshake
succeed?" check that doesn't account for it.

---

## ADR-006: Fixed pairing port with a scoped Windows installer firewall rule

**Status:** Accepted

**Decision:** CoSync pairing uses UDP port 48215. The Windows NSIS
installer runs per-machine and adds a firewall exception restricted to the
installed CoSync executable, that UDP port, Private profiles, and the local
subnet. The uninstaller removes the rule.

**Measured justification:** On the primary Windows development host, all
Windows Firewall profiles block inbound traffic by default and no CoSync rule
was present. The pairing QR reached a real Wi-Fi address, but Android QUIC
attempts still timed out. An ephemeral listener port cannot be permitted
without a broad inbound-UDP rule, so a stable port enables the minimal rule.

**Cross-platform impact:** The pairing port is protocol-level and identical
on Windows and Linux. Only Windows installer automation differs: Linux
packages do not modify host firewall policy and must rely on the distribution
or administrator's normal firewall management.
