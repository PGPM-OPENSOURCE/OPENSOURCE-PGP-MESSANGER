# PGPM for macOS — written in Rust

The macOS version of PGPM: a native app written in Rust. It is the same app as the Python / Tk
one (files/), in its console look, with GnuPG as the engine. It reads and writes the same settings
(`~/.pgp_messenger/config.json`: active key, contact order, nicknames) and the same GnuPG
keyring, so either version can be used on the same data.

- UI: [egui](https://github.com/emilk/egui) / eframe, drawn to match the Tk console look
  (Cascadia Mono 16px on 19px lines, bracketed words, inverted selection)
- Keys and messages: the `gpg` binary (GPG Suite / Homebrew), so PINs and passphrases are
  asked by gpg's own pinentry and the keyring stays shared with `gpg` and other tools
- Network: [ureq](https://github.com/algesten/ureq) for the update check and for checking a
  smartcard's public-key URL
- macOS: the menu bar (About PGPM, Edit, Window), a dark title bar, files dropped onto the
  message boxes

## Build

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
| `app/` | the app: sidebar (`sidebar.rs`), panels (`panels.rs`, `card.rs`, `profile.rs`), dialogs (`dialog.rs`), the console toolkit (`ui.rs`), macOS integration (`macos.rs`) |
| `scripts/` | packaging |
| `installer/` | the setup package: its choices (`distribution.xml`), pages and install scripts |

## Tests

```sh
cargo test --workspace                      # core: key listing, settings, versions
cargo test -p pgpm-core -- --ignored        # online: GitHub release check, key-URL checks
```

`pgpm --snap <scenario> --out <dir>` opens one screen in a known state (for comparing with
the Tk app's screens); `pgpm --script <file> --out <dir>` drives the running app with
pointer, keyboard and file-drop input (see `app/src/script.rs`).

## License

GPL-3.0, see [LICENSE](../LICENSE). Cascadia Mono is under the SIL Open Font License 1.1
(`fonts/CascadiaMono-OFL.txt`).
