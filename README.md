# PGPM

[![Download latest release](https://img.shields.io/badge/Download-Latest%20Release-2ea44f?style=for-the-badge&logo=github)](../../releases/latest)

A modern, dark-themed **Windows** desktop app for working with OpenPGP keys and messages — generate
and manage keys, sign / encrypt, decrypt / verify, manage contacts' public keys, and use a hardware
OpenPGP smartcard (Nitrokey / YubiKey). Built with Python + Tkinter; the GnuPG keyring is the source
of truth (so it stays in sync with tools like Kleopatra / `gpg`).

## Download & run

No installation needed — a single self-contained file you **double-click to open**. Grab it from the
[Releases](../../releases/latest) page (or the button above):

| Download | Run it |
|----------|--------|
| `PGPM.exe` | Double-click. First launch: Windows SmartScreen → *More info → Run anyway* (unsigned). |

> ℹ️ **GnuPG is the only prerequisite** — install [Gpg4win](https://www.gpg4win.org) once, then the
> app just runs. Without it the app still opens but key/crypto features are disabled.

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
Result: `dist/PGPM.exe`. (CI builds this automatically on each `vX.Y.Z` tag and attaches it to a
Release.)

## Notes
- Private keys never leave your GnuPG keyring; the app's own data dir is `~/.pgp_messenger`.
