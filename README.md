# PGPM

[![Download latest release](https://img.shields.io/badge/Download-Latest%20Release-2ea44f?style=for-the-badge&logo=github)](../../releases/latest)

desktop app for working with OpenPGP keys and messages — generate
and manage keys, sign / encrypt, decrypt / verify, manage contacts' public keys, and use a hardware
OpenPGP smartcard (Nitrokey / YubiKey). **Windows** and **macOS** run one native app written in
**Rust** ([`rust/`](rust/)); the GnuPG keyring is the source of truth (so it stays in sync with tools
like Kleopatra / `gpg`).

## Download & run

Grab the setup for your system from the [Releases](../../releases/latest) page (or the button above):

| Download | Install |
|----------|---------|
| `PGPM-Setup.exe` | Windows, native app written in Rust. Double-click and follow the setup; it installs GnuPG too if the PC doesn't have it. First launch: Windows SmartScreen → *More info → Run anyway* (unsigned). |
| `PGPM-Setup.pkg` | macOS, native app written in Rust. Double-click and follow the setup; it gets GnuPG ([GPG Suite](https://gpgtools.org)) too if the Mac doesn't have it. First time: right-click it → *Open* (not signed by Apple; on macOS 15 and later: System Settings → Privacy & Security → *Open Anyway*). |

> ℹ️ **GnuPG is the only prerequisite**, and both setups install it when it's missing. Without it
> the app still opens but key/crypto features are disabled.

## Features

- RSA-4096 key generation, import/export (armored or `.asc`), contacts from your public keyring.
- Sign / encrypt and decrypt / verify; per-contact chat-style compose box.
- Encrypt-to-self on every message, key-expiry badges, and file import/drag-and-drop into the compose box.
- Hardware smartcard (Nitrokey / YubiKey) operations via GnuPG.

## Build the Windows app (Rust)

```bash
cd rust
cargo build -p pgpm --release   # target/release/pgpm.exe
```
Requires Rust (stable) and the Microsoft C++ Build Tools (for the linker; nothing in PGPM is C).
The setup is built with
[Inno Setup](https://jrsoftware.org/isinfo.php) from `installer/PGPM.iss` (copy `pgpm.exe` to
`dist/PGPM.exe` first). CI builds both on each `vX.Y.Z` tag and attaches `PGPM-Setup.exe` to the
Release.

## Build the macOS app (Rust)

```bash
cd rust
scripts/package-macos.sh        # dist/PGPM.app, dist/PGPM-macOS.dmg and dist/PGPM-Setup.pkg
```
Requires Rust (stable). See [`rust/README.md`](rust/README.md).

## Notes
- Private keys never leave your GnuPG keyring; the app's own data dir is `~/.pgp_messenger`.
