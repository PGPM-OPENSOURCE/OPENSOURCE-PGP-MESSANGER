//! The network: the update check (GitHub's latest release) and checking a smartcard's
//! public-key URL before it is written to the card.

use std::collections::BTreeSet;
use std::io::Read;
use std::time::Duration;

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Why a download failed, in the words Python's urllib gave (the dialogs quote it).
fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::HostNotFound => "nodename nor servname provided, or not known".into(),
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::Io(io) => match io.kind() {
            std::io::ErrorKind::ConnectionRefused => "Connection refused".into(),
            std::io::ErrorKind::TimedOut => "timed out".into(),
            _ => io.to_string().trim_start_matches("failed to lookup address information: ").to_string(),
        },
        other => other.to_string().trim_start_matches("failed to lookup address information: ").to_string(),
    }
}

/// Up to `max` bytes from `url`; on failure the "(why, HTTP code)" text of the error dialog.
pub fn download(url: &str, max: u64, timeout: u64) -> Result<Vec<u8>, String> {
    let resp = agent(timeout).get(url).header("User-Agent", "PGPM").call().map_err(|e| describe(&e))?;
    let status = resp.status();
    if !status.is_success() {
        let reason = status.canonical_reason().unwrap_or("Error");
        return Err(format!("{reason}, HTTP {}", status.as_u16()));
    }
    let mut buf = Vec::new();
    resp.into_body().into_reader().take(max).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// "1.10.2" > "1.9" (digits of each dot-separated part, missing parts = 0).
pub fn version_newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.split('.')
            .map(|p| {
                let d: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
                d.parse().unwrap_or(0)
            })
            .collect()
    }
    parts(a) > parts(b)
}

/// The latest release's version when it is newer than this one (offline / private repo: None).
pub fn newer_release() -> Option<String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", crate::GITHUB_REPO);
    let body = download(&url, 1 << 20, 6).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    let tag = v.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    (!tag.is_empty() && version_newer(&tag, crate::APP_VERSION)).then_some(tag)
}

/// Why `url` won't give GnuPG this card's public key (`fprs` = the card's key fingerprints),
/// or None if it will.
pub fn key_url_problem(url: &str, fprs: &BTreeSet<String>) -> Option<String> {
    let re = regex::Regex::new(r"(?i)^https?://\S+$").unwrap();
    if !re.is_match(url) {
        return Some("That isn't a web address — it has to start with https://.".into());
    }
    let data = match download(url, 1 << 20, 15) {
        Ok(d) => d,
        Err(why) => return Some(format!("Couldn't download it ({why}).")),
    };
    let mut tmp = match tempfile::Builder::new().suffix(".asc").tempfile() {
        Ok(t) => t,
        Err(e) => return Some(format!("Couldn't check it: {e}")),
    };
    if let Err(e) = std::io::Write::write_all(&mut tmp, &data) {
        return Some(format!("Couldn't check it: {e}"));
    }
    let r = match crate::gpg::show_keys(tmp.path()) {
        Ok(r) => r,
        Err(e) => return Some(format!("Couldn't check it: {e}")),
    };
    let recs: Vec<Vec<&str>> = r.stdout.lines().map(|l| l.split(':').collect()).collect();
    let have: BTreeSet<String> =
        recs.iter().filter(|f| f[0] == "fpr" && f.len() > 9).map(|f| f[9].to_string()).collect();
    if have.is_empty() {
        return Some("There's no PGP public key at that address.".into());
    }
    if !fprs.is_empty() && fprs.is_disjoint(&have) {
        return Some("The key at that address isn't this card's key.".into());
    }
    if !recs.iter().any(|f| f[0] == "uid") {
        return Some(
            "The key there has no name or email (user ID), so GnuPG can't import it — \
             keys.openpgp.org leaves those out until you confirm the address there."
                .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(version_newer("0.6.3", "0.6.2"));
        assert!(version_newer("1.0", "0.9.9"));
        assert!(!version_newer("0.6.2", "0.6.2"));
        assert!(version_newer("0.10.0", "0.9.0"));
        assert!(!version_newer("0.6", "0.6.0"));
    }
}
