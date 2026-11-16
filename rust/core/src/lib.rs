//! PGPM's engine without the UI: GnuPG (the keyring is the source of truth, shared with
//! `gpg` and Kleopatra), the keys it lists, PGPM's own settings in `~/.pgp_messenger`, and the
//! two network calls (update check, a smartcard's public-key URL).

pub mod gpg;
pub mod keys;
pub mod net;
pub mod store;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "PGPM-OPENSOURCE/OPENSOURCE-PGP-MESSANGER";

/// The GnuPG distribution the app points users to.
#[cfg(target_os = "macos")]
pub const GPG_DIST: (&str, &str) = ("GPG Suite", "https://gpgtools.org");
#[cfg(not(target_os = "macos"))]
pub const GPG_DIST: (&str, &str) = ("Gpg4win", "https://www.gpg4win.org");
