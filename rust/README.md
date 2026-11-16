# PGPM for Windows and macOS — written in Rust

PGPM on Windows and macOS: one native app written in Rust, in its console look, with GnuPG as
the engine. It keeps the settings the earlier Python version used (`~/.pgp_messenger/config.json`:
active key, contact order, nicknames) and the GnuPG keyring, so an update keeps everything.

- UI: [egui](https://github.com/emilk/egui) / eframe in PGPM's console look
  (Cascadia Mono 16px on 19px lines, bracketed words, inverted selection)
- Keys and messages: the `gpg` binary (GnuPG / Gpg4win on Windows, GPG Suite / Homebrew on
  macOS), so PINs and passphrases are asked by gpg's own pinentry and the keyring stays shared
  with `gpg` and other tools
- Network: [ureq](https://github.com/algesten/ureq) for the update check and for checking a
  smartcard's public-key URL, over rustls with [RustCrypto](https://github.com/RustCrypto/rustls-rustcrypto)'s
  pure-Rust algorithms and Mozilla's root certificates
- Nothing in PGPM is compiled from C or assembly: every crate it builds is Rust (GnuPG is a
  separate program it runs)
- macOS: the menu bar (About PGPM, Edit, Window), a dark title bar, files dropped onto the
  message boxes
- Windows: a dark title bar, files dropped onto the message boxes, one PGPM at a time (the
  `PGPM_single_instance_mutex` the setup checks), GnuPG found on the PATH or where its
  installer registered it (never Git's own gpg), the icon and version info in PGPM.exe, and the
  C runtime linked in (`.cargo/config.toml`), so it needs no Visual C++ Redistributable

## Build on Windows

From this folder (`rust/`), with Rust (stable) and the Microsoft C++ Build Tools (the linker):

```sh
cargo build -p pgpm --release        # target/release/pgpm.exe
```

The setup (`PGPM-Setup.exe`, with GnuPG bundled) is `installer/PGPM.iss` at the repository root,
built with Inno Setup from `dist/PGPM.exe`; CI does all of it on each `vX.Y.Z` tag.

## Build on macOS

From this folder (`rust/`):

```sh
cargo build -p pgpm --release        # target/release/pgpm
scripts/package-macos.sh             # dist/PGPM.app, dist/PGPM-macOS.dmg and dist/PGPM-Setup.pkg
```

`PGPM-Setup.pkg` is the macOS counterpart of the Windows setup: it installs PGPM in Applications,
with an optional desktop icon, and when the Mac has no GnuPG it downloads GPG Suite from
gpgtools.org (a pinned release, checked against its SHA-256 and its signature) and opens its
installer.

The font (Cascadia Mono) and the icon come from the repository's `fonts/` and `assets/`.

The app is signed ad hoc (no Apple Developer ID), so the first time macOS asks: right-click
the app, choose **Open**, then **Open**. GnuPG is the only prerequisite
([GPG Suite](https://gpgtools.org)).

## Layout

| Path | What |
| --- | --- |
| `core/` | GnuPG (keyring listing, import / export / delete, generate, sign, encrypt, decrypt, smartcard commands), `~/.pgp_messenger` settings, the update check and key-URL check |
| `app/` | the app: sidebar (`sidebar.rs`), panels (`panels.rs`, `card.rs`, `profile.rs`), dialogs (`dialog.rs`), the console toolkit (`ui.rs`), macOS integration (`macos.rs`), Windows integration (`win.rs`, `build.rs`) |
| `scripts/` | packaging |
| `installer/` | the setup package: its choices (`distribution.xml`), pages and install scripts |

## Tests

```sh
cargo test --workspace                      # core: key listing, settings, versions
cargo test -p pgpm-core -- --ignored        # online: GitHub release check, key-URL checks
```

`pgpm --snap <scenario> --out <dir>` opens one screen in a known state (for screenshots);
`pgpm --script <file> --out <dir>` drives the running app with
pointer, keyboard and file-drop input (see `app/src/script.rs`).

## License

GPL-3.0, see [LICENSE](../LICENSE). Cascadia Mono is under the SIL Open Font License 1.1
(`fonts/CascadiaMono-OFL.txt`).
