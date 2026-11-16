//! Network checks against the real servers (ignored by default: `cargo test -- --ignored`).

use std::collections::BTreeSet;

#[test]
#[ignore]
fn github_latest_release_is_readable() {
    let url = format!("https://api.github.com/repos/{}/releases/latest", pgpm_core::GITHUB_REPO);
    let body = pgpm_core::net::download(&url, 1 << 20, 10).expect("download");
    let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let tag = v["tag_name"].as_str().expect("tag_name");
    println!("latest release: {tag}; newer than this build: {:?}", pgpm_core::net::newer_release());
}

#[test]
#[ignore]
fn key_url_checks() {
    let none = BTreeSet::new();
    let bad = pgpm_core::net::key_url_problem("1412r3f3f23", &none).unwrap();
    assert_eq!(bad, "That isn't a web address — it has to start with https://.");
    let host = pgpm_core::net::key_url_problem("https://no-such-host.invalid/key.asc", &none).unwrap();
    println!("unknown host: {host}");
    assert!(host.starts_with("Couldn't download it ("));
    let missing = pgpm_core::net::key_url_problem("https://keys.openpgp.org/vks/v1/by-fingerprint/0000000000000000000000000000000000000000", &none);
    println!("missing key: {missing:?}");
    let fprs: BTreeSet<String> = ["0D3E670CBB5B237FCA3DECD1AD0405C434EC0254".to_string()].into();
    let ubuntu = pgpm_core::net::key_url_problem(
        "https://keyserver.ubuntu.com/pks/lookup?op=get&options=mr&search=0x0D3E670CBB5B237FCA3DECD1AD0405C434EC0254",
        &fprs,
    );
    println!("keyserver.ubuntu.com (the Nitrokey's key): {ubuntu:?}");
    let wrong: BTreeSet<String> = ["FFFF".to_string()].into();
    let other = pgpm_core::net::key_url_problem(
        "https://keyserver.ubuntu.com/pks/lookup?op=get&options=mr&search=0x0D3E670CBB5B237FCA3DECD1AD0405C434EC0254",
        &wrong,
    );
    println!("another card's key: {other:?}");
}
