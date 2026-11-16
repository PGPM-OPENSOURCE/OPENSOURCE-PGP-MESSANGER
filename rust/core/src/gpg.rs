//! GnuPG: everything PGPM does with keys and messages goes through the `gpg` binary, so the
//! keyring stays shared with `gpg` itself and Kleopatra, and PINs / passphrases are asked by
//! gpg's own pinentry.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::keys::{Key, parse_colons};

/// What a gpg run left behind.
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
    /// stderr, else `what`, trimmed (the error a failed call reports).
    fn err(&self, what: &str) -> String {
        let e = if self.stderr.is_empty() { what } else { &self.stderr };
        e.trim().to_string()
    }
}

/// The `gpg` executable: on the PATH, else (an app started from Finder only gets
/// /usr/bin:/bin:/usr/sbin:/sbin) GPG Suite's and Homebrew's install dirs.
pub fn path() -> Option<PathBuf> {
    for name in ["gpg", "gpg2"] {
        if let Some(p) = which(name) {
            return Some(p);
        }
    }
    #[cfg(target_os = "macos")]
    for exe in ["/usr/local/MacGPG2/bin/gpg", "/opt/homebrew/bin/gpg", "/usr/local/bin/gpg"] {
        if is_exe(Path::new(exe)) {
            return Some(exe.into());
        }
    }
    #[cfg(windows)]
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(d) = std::env::var_os(env) {
            let exe = PathBuf::from(d).join("GnuPG").join("bin").join("gpg.exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let p = dir.join(name);
        if is_exe(&p) {
            return Some(p);
        }
        #[cfg(windows)]
        {
            let p = dir.join(format!("{name}.exe"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_exe(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_exe(p: &Path) -> bool {
    p.is_file()
}

pub fn available() -> bool {
    path().is_some()
}

/// Runs gpg with `args` (stdin fed `stdin`, or empty). An app started from Finder has no
/// terminal: without --no-tty gpg's card and key editors die on "cannot open '/dev/tty'".
pub fn run(args: &[&str], stdin: Option<&str>, timeout: Duration) -> Result<Output, String> {
    let g = path().ok_or_else(|| "GnuPG (gpg) is not installed.".to_string())?;
    let mut cmd = Command::new(&g);
    if !cfg!(windows) {
        cmd.arg("--no-tty");
    }
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut sin = child.stdin.take();
    let input = stdin.unwrap_or("").to_string();
    let writer = std::thread::spawn(move || {
        if let Some(s) = sin.as_mut() {
            let _ = s.write_all(input.as_bytes());
        }
        drop(sin); // EOF
    });
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let ro = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let re = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Command '{} {}' timed out after {} seconds",
                        g.display(),
                        args.join(" "),
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let _ = writer.join();
    let out = ro.join().unwrap_or_default();
    let err = re.join().unwrap_or_default();
    Ok(Output {
        code: status.code(),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    })
}

const T: Duration = Duration::from_secs(120);

fn run_t(args: &[&str], stdin: Option<&str>) -> Result<Output, String> {
    run(args, stdin, T)
}

// -------------------------------------------------------------------------------------------
// Smartcard
// -------------------------------------------------------------------------------------------
/// `gpg --card-status`, the fields PGPM shows (`present` false without a card).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardInfo {
    pub present: bool,
    pub error: Option<String>,
    /// serial, name, version, sig_fpr, enc_fpr, auth_fpr, pin_retries, reader, url
    pub fields: BTreeMap<String, String>,
}

impl CardInfo {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.fields.get(k).map(String::as_str).filter(|v| !v.is_empty())
    }
}

pub fn card_status() -> CardInfo {
    let r = match run_t(&["--card-status"], None) {
        Ok(r) => r,
        Err(e) => return CardInfo { present: false, error: Some(e), ..Default::default() },
    };
    let low = format!("{}\n{}", r.stdout, r.stderr).to_lowercase();
    if low.contains("no such device") || low.contains("card not available") || low.contains("selecting card failed") {
        return CardInfo::default();
    }
    let labels = [
        ("Serial number", "serial"),
        ("Name of cardholder", "name"),
        ("Version", "version"),
        ("Signature key", "sig_fpr"),
        ("Encryption key", "enc_fpr"),
        ("Authentication key", "auth_fpr"),
        ("PIN retry counter", "pin_retries"),
        ("Reader", "reader"),
        ("URL of public key", "url"),
    ];
    let mut info = CardInfo { present: true, ..Default::default() };
    for line in r.stdout.lines() {
        let Some((head, val)) = line.split_once(':') else { continue };
        let head = head.trim_end_matches(['.', ' ']).trim();
        let mut val = val.trim().to_string();
        if let Some((_, key)) = labels.iter().find(|(l, _)| *l == head) {
            if key.ends_with("_fpr") {
                val = val.replace(' ', "");
            }
            info.fields.insert(key.to_string(), val);
        }
    }
    info
}

/// Primary-key fingerprint of the secret key that owns `fpr` (a subkey, e.g. the card's
/// encryption key, or the primary itself); `fpr` if gpg doesn't know.
pub fn primary_fpr(fpr: &str) -> String {
    let Ok(r) = run_t(&["--with-colons", "--list-secret-keys", fpr], None) else { return fpr.into() };
    for line in r.stdout.lines() {
        let f: Vec<&str> = line.split(':').collect();
        if f[0] == "fpr" && f.len() > 9 && !f[9].is_empty() {
            return f[9].to_string();
        }
    }
    fpr.into()
}

fn card_run(stdin: &str, edit: bool) -> Result<Output, String> {
    let sub = if edit { "--card-edit" } else { "--change-pin" };
    run(&["--command-fd", "0", "--status-fd", "2", sub], Some(stdin), Duration::from_secs(180))
}

/// 1 = user PIN, 2 = unblock (reset code / admin), 3 = admin PIN
pub fn card_change_pin(which: &str) -> Result<Output, String> {
    card_run(&format!("{which}\nQ\n"), false)
}

pub fn card_set_name(surname: &str, given: &str) -> Result<Output, String> {
    card_run(&format!("admin\nname\n{surname}\n{given}\nquit\n"), true)
}

pub fn card_set_url(url: &str) -> Result<Output, String> {
    card_run(&format!("admin\nurl\n{url}\nquit\n"), true)
}

pub fn card_fetch() -> Result<Output, String> {
    card_run("fetch\nquit\n", true)
}

/// Fetches a public key by fingerprint from a keyserver into the keyring.
pub fn recv_key(fpr: &str) -> Result<Output, String> {
    run(&["--keyserver", "hkps://keyserver.ubuntu.com", "--recv-keys", fpr], None, Duration::from_secs(60))
}

/// Moves the (sub)key into a card slot: 1 = sign, 2 = encrypt, 3 = auth.
pub fn keytocard(key_fpr: &str, slot: &str) -> Result<Output, String> {
    let cmds = format!("keytocard\n{slot}\nsave\n");
    run(&["--command-fd", "0", "--status-fd", "2", "--edit-key", key_fpr], Some(&cmds), Duration::from_secs(180))
}

/// Lists the keys in a file without importing them.
pub fn show_keys(file: &Path) -> Result<Output, String> {
    let p = file.to_string_lossy().into_owned();
    run_t(&["--with-colons", "--show-keys", &p], None)
}

// -------------------------------------------------------------------------------------------
// Keyring
// -------------------------------------------------------------------------------------------
fn list(secret: bool) -> Vec<Key> {
    let flag = if secret { "--list-secret-keys" } else { "--list-keys" };
    match run_t(&["--with-colons", "--fixed-list-mode", flag], None) {
        Ok(r) => parse_colons(&r.stdout),
        Err(_) => Vec::new(),
    }
}

pub fn list_secret() -> Vec<Key> {
    list(true)
}

pub fn list_public() -> Vec<Key> {
    list(false)
}

pub fn import_key(armored: &str) -> Result<String, String> {
    let r = run_t(&["--import", "--status-fd", "1"], Some(armored))?;
    let out = format!("{}{}", r.stdout, r.stderr);
    let low = out.to_lowercase();
    if !r.ok() && !out.contains("IMPORT_OK") && !low.contains("imported") && !low.contains("unchanged") {
        return Err(r.err("import failed"));
    }
    Ok(out)
}

/// The armored public key ("" when gpg has none).
pub fn export_pub(fpr: &str) -> String {
    if fpr.is_empty() {
        return String::new();
    }
    match run_t(&["--armor", "--export", fpr], None) {
        Ok(r) => r.stdout.trim().to_string(),
        Err(_) => String::new(),
    }
}

pub fn export_secret(fpr: &str) -> Result<String, String> {
    let r = run_t(&["--armor", "--export-secret-keys", fpr], None)?;
    if !r.ok() || r.stdout.is_empty() {
        return Err(r.err("export failed"));
    }
    Ok(r.stdout)
}

pub fn delete_pub(fpr: &str) -> Result<(), String> {
    let r = run_t(&["--batch", "--yes", "--delete-keys", fpr], None)?;
    if !r.ok() {
        return Err(r.err("delete failed"));
    }
    Ok(())
}

pub fn delete_secret(fpr: &str) -> Result<(), String> {
    let r = run_t(&["--batch", "--yes", "--delete-secret-and-public-key", fpr], None)?;
    if !r.ok() {
        return Err(r.err("delete failed"));
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// Generate / sign / encrypt / decrypt
// -------------------------------------------------------------------------------------------
/// An RSA-4096 key pair that never expires (passphrase-protected when one is given).
pub fn generate(name: &str, email: &str, passphrase: &str) -> Result<String, String> {
    let mut params = vec![
        "Key-Type: RSA".to_string(),
        "Key-Length: 4096".into(),
        "Subkey-Type: RSA".into(),
        "Subkey-Length: 4096".into(),
        format!("Name-Real: {name}"),
    ];
    if !email.is_empty() {
        params.push(format!("Name-Email: {email}"));
    }
    params.push("Expire-Date: 0".into());
    params.push(if passphrase.is_empty() { "%no-protection".into() } else { format!("Passphrase: {passphrase}") });
    params.push("%commit".into());
    let input = params.join("\n") + "\n";
    let r = run(&["--batch", "--status-fd", "2", "--gen-key"], Some(&input), Duration::from_secs(600))?;
    if !r.ok() {
        return Err(r.err("key generation failed"));
    }
    Ok(format!("{}{}", r.stdout, r.stderr))
}

/// A clear-signed copy of `text` (signed with `fpr` when given).
pub fn sign(text: &str, fpr: Option<&str>) -> Result<String, String> {
    let mut args = vec!["--armor", "--clearsign"];
    if let Some(f) = fpr {
        args.extend(["--local-user", f]);
    }
    let r = run_t(&args, Some(text))?;
    if !r.ok() || r.stdout.is_empty() {
        return Err(r.err("signing failed"));
    }
    Ok(r.stdout)
}

pub fn encrypt(text: &str, recipients: &[String], sign_fpr: Option<&str>) -> Result<String, String> {
    let mut args: Vec<&str> = vec!["--armor", "--encrypt", "--always-trust"];
    for r in recipients {
        args.extend(["--recipient", r]);
    }
    if let Some(f) = sign_fpr {
        args.extend(["--sign", "--local-user", f]);
    }
    let r = run_t(&args, Some(text))?;
    if !r.ok() || r.stdout.is_empty() {
        return Err(r.err("encryption failed"));
    }
    Ok(r.stdout)
}

/// Decrypts (and verifies) with the keyring or the card: (plaintext, signer's user ID).
pub fn decrypt(armored: &str) -> Result<(String, Option<String>), String> {
    let r = run_t(&["--decrypt", "--status-fd", "2"], Some(armored))?;
    if !r.ok() && r.stdout.is_empty() {
        return Err(r.err("decryption failed"));
    }
    let signer = r
        .stderr
        .lines()
        .find(|l| l.contains("Good signature from") && l.contains('"'))
        .and_then(|l| l.split('"').nth(1))
        .map(str::to_string);
    Ok((r.stdout, signer))
}
