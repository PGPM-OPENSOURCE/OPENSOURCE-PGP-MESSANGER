//! PGPM's own files in `~/.pgp_messenger` (the same ones the Python app uses, so either
//! version can open the other's settings): config.json (active key, contact order,
//! nicknames), debug.log, and the legacy contacts.json / keys.json migrated into GnuPG once.

use std::io::Write;
use std::path::PathBuf;

use serde_json::{Map, Value};

pub fn data_dir() -> PathBuf {
    // PGPM_DATA_DIR: another folder for these files (tests run on a throwaway one; moving HOME /
    // USERPROFILE instead would move GnuPG's agent socket too)
    if let Some(d) = std::env::var_os("PGPM_DATA_DIR").filter(|d| !d.is_empty()) {
        let d = PathBuf::from(d);
        let _ = std::fs::create_dir_all(&d);
        return d;
    }
    // `~` as Python's os.path.expanduser sees it: %USERPROFILE% on Windows (which has no HOME)
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    let home = home.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let d = home.join(".pgp_messenger");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

/// Appends a diagnostic line to debug.log (best effort).
pub fn log(msg: &str) {
    let line = format!("{}  {msg}\n", chrono::Local::now().format("%H:%M:%S"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("debug.log")) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn load(path: &PathBuf) -> Option<Value> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

/// JSON the way Python's `json.dump(data, f, indent=2)` writes it: two-space indent and
/// non-ASCII characters escaped as \uXXXX.
pub fn to_python_json(v: &Value) -> String {
    let s = serde_json::to_string_pretty(v).unwrap_or_default();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{:04x}", u));
            }
        }
    }
    out
}

pub fn config() -> Map<String, Value> {
    match load(&config_path()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

pub fn save_config(cfg: &Map<String, Value>) {
    let _ = std::fs::write(config_path(), to_python_json(&Value::Object(cfg.clone())));
}

pub fn cfg_get(key: &str) -> Option<Value> {
    config().get(key).cloned()
}

pub fn cfg_set(key: &str, value: Value) {
    let mut c = config();
    c.insert(key.to_string(), value);
    save_config(&c);
}

/// The active key's fingerprint.
pub fn my_fpr() -> Option<String> {
    cfg_get("my_fpr").and_then(|v| v.as_str().map(str::to_string))
}

pub fn set_my_fpr(fpr: Option<&str>) {
    cfg_set("my_fpr", fpr.map_or(Value::Null, |f| Value::String(f.into())));
    log(&format!("active key -> {}", fpr.unwrap_or("None")));
}

/// The manual drag order of the contacts (fingerprints).
pub fn contact_order() -> Vec<String> {
    match cfg_get("contact_order") {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

pub fn set_contact_order(order: &[String]) {
    cfg_set("contact_order", Value::Array(order.iter().map(|f| Value::String(f.clone())).collect()));
}

/// Local nicknames by fingerprint (the key in the keyring is never changed).
pub fn nicknames() -> Map<String, Value> {
    match cfg_get("nicknames") {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

pub fn nick(fpr: &str) -> String {
    nicknames().get(fpr).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub fn set_nick(fpr: &str, nick: &str) {
    let mut d = nicknames();
    if nick.is_empty() {
        d.remove(fpr);
    } else {
        d.insert(fpr.to_string(), Value::String(nick.to_string()));
    }
    cfg_set("nicknames", Value::Object(d));
}

/// Imports keys an old version kept in keys.json / contacts.json into GnuPG, once.
pub fn migrate_legacy() {
    let mut cfg = config();
    if cfg.get("migrated").and_then(Value::as_bool).unwrap_or(false) || !crate::gpg::available() {
        return;
    }
    let dir = data_dir();
    if let Some(Value::Object(kd)) = load(&dir.join("keys.json")) {
        for k in ["private", "public"] {
            if let Some(blob) = kd.get(k).and_then(Value::as_str) {
                if blob.contains("BEGIN PGP") {
                    let _ = crate::gpg::import_key(blob);
                }
            }
        }
    }
    if let Some(Value::Array(cs)) = load(&dir.join("contacts.json")) {
        for c in cs {
            if let Some(pk) = c.get("public_key").and_then(Value::as_str) {
                if pk.contains("BEGIN PGP") {
                    let _ = crate::gpg::import_key(pk);
                }
            }
        }
    }
    cfg.insert("migrated".into(), Value::Bool(true));
    save_config(&cfg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_json() {
        let v = serde_json::json!({"migrated": true, "my_fpr": null, "contact_order": ["A", "B"],
                                   "nicknames": {"X": "Bjørn"}, "e": []});
        assert_eq!(
            to_python_json(&v),
            "{\n  \"migrated\": true,\n  \"my_fpr\": null,\n  \"contact_order\": [\n    \"A\",\n    \"B\"\n  ],\n  \"nicknames\": {\n    \"X\": \"Bj\\u00f8rn\"\n  },\n  \"e\": []\n}"
        );
    }
}
