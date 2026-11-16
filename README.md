# PGPM

[![Download latest release](https://img.shields.io/badge/Download-Latest%20Release-2ea44f?style=for-the-badge&logo=github)](../../releases/latest)

A modern, dark-themed **Windows** desktop app for working with OpenPGP keys and messages — generate
and manage keys, sign / encrypt, decrypt / verify, manage contacts' public keys, and use a hardware
OpenPGP smartcard (Nitrokey / YubiKey). Built with Python + Tkinter; the GnuPG keyring is the source
of truth (so it stays in sync with tools like Kleopatra / `gpg`). The **macOS** version is a native
app written in **Rust** ([`rust/`](rust/)), with the same look and the same keyring and settings.

## Download & run

Grab the setup for your system from the [Releases](../../releases/latest) page (or the button above):

| Download | Install |
|----------|---------|
| `PGPM-Setup.exe` | Windows. Double-click and follow the setup; it installs GnuPG too if the PC doesn't have it. First launch: Windows SmartScreen → *More info → Run anyway* (unsigned). |
| `PGPM-Setup.pkg` | macOS, native app written in Rust. Double-click and follow the setup; it gets GnuPG ([GPG Suite](https://gpgtools.org)) too if the Mac doesn't have it. First time: right-click it → *Open* (not signed by Apple; on macOS 15 and later: System Settings → Privacy & Security → *Open Anyway*). |

> ℹ️ **GnuPG is the only prerequisite**, and both setups install it when it's missing. Without it
> the app still opens but key/crypto features are disabled.

## Features

- RSA-4096 key generation, import/export (armored or `.asc`), contacts from your public keyring.
- Sign / encrypt and decrypt / verify; per-contact chat-style compose box.
- Encrypt-to-self on every message, key-expiry badges, and file import/drag-and-drop into the compose box.
- Hardware smartcard (Nitrokey / YubiKey) operations via GnuPG.

## Run from source

```bash
pip install -r files/requirements.txt
python files/pgp_messenger.py
```
Requires Python 3.10+ with Tkinter (bundled with the official python.org installer).

## Build the standalone yourself

```bash
pip install -r files/requirements.txt pyinstaller
pyinstaller --onefile --windowed --name PGPM --icon assets/icon.ico \
  --add-data "assets;assets" --add-data "fonts;fonts" files/pgp_messenger.py
```
Result: `dist/PGPM.exe`. (CI builds it and the setup, `PGPM-Setup.exe`, on each `vX.Y.Z` tag and
attaches the setup to the Release.)

## Build the macOS app (Rust)

```bash
cd rust
scripts/package-macos.sh        # dist/PGPM.app, dist/PGPM-macOS.dmg and dist/PGPM-Setup.pkg
```
Requires Rust (stable). See [`rust/README.md`](rust/README.md).

## Notes
- Private keys never leave your GnuPG keyring; the app's own data dir is `~/.pgp_messenger`.
