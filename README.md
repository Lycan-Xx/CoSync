[![CoSync Build](https://github.com/Lycan-Xx/CoSync/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/Lycan-Xx/CoSync/actions/workflows/build.yml)

# CoSync

A cross-platform device mesh: clipboard, files, and phone accessibility
features synced silently between Android and PC over your local network.
No cloud, no account, no relay server — everything stays on your LAN.

## Vision

Not just a clipboard manager — a persistent local sync layer between your
phone and your desktop. Copy on one, paste on the other. Drag a file from
your PC and it's on your phone's gallery a second later. Eventually:
notification mirroring, quick replies from the desktop, and (further out)
using your phone's camera as a PC webcam — all over a direct, encrypted,
device-to-device connection.

## Philosoph

Cosync should not look like just another application that manages device synchronization. It should make device synchronization feel like part of the operating system.

**Priority order:** Android → Windows first, Linux second, iOS only if
real demand shows up. macOS is intentionally out of scope — see
[`docs/DECISIONS.md`](docs/DECISIONS.md).

## Status

Milestone 0 (repo scaffolding) is complete. See:
- [`docs/SPEC.md`](docs/SPEC.md) — the full phased development spec
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — architecture decisions and why

## Architecture

| Layer | Choice |
|---|---|
| Shared core | Rust (`crates/core`) |
| Desktop | Tauri v2 + React/TypeScript (`apps/desktop`) |
| Mobile | React Native, Expo bare workflow (`apps/mobile`) |
| Rust ↔ Mobile bridge | `uniffi-rs` → Kotlin bindings → native module |
| Discovery | mDNS |
| Transport | QUIC (`quinn`), TLS 1.3, pinned self-signed certs |
| Payloads | Protobuf (`prost`) |
| Storage | SQLite (`rusqlite`) |

## Getting Started

\`\`\`
# Rust workspace
cargo build

# Desktop shell
cd apps/desktop && npm install && npm run tauri dev

# Mobile shell (already run through \`expo prebuild\`, not Expo Go)
cd apps/mobile && npm install && npx expo run:android
\`\`\`

## Mobile shell Vision
The phone app is not a companion to the desktop it is the trust surface for the desktop, and almost everything else the user does with CoSync on mobile should happen in Android's own surfaces, not inside CoSync's.
## License

MIT — see [\`LICENSE\`](LICENSE).
