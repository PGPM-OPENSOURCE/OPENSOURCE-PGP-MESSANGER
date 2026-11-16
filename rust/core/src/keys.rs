//! Keys as `gpg --with-colons --fixed-list-mode` lists them.

use chrono::{DateTime, Local, TimeZone};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Key {
    /// primary key fingerprint
    pub fpr: String,
    pub keyid: String,
    /// seconds since the epoch
    pub created: Option<i64>,
    pub expires: Option<i64>,
    /// from the first user ID
    pub name: String,
    pub email: String,
    pub uids: Vec<String>,
    pub secret: bool,
    /// its private part lives on an inserted smartcard
    pub on_card: bool,
    /// colon field 2 ("r" = revoked, "e" = expired, ...)
    pub validity: String,
    /// "E" in the key's capabilities: it can still encrypt (not expired, revoked or sign-only);
    /// gpg refuses the others as "Unusable public key"
    pub can_encrypt: bool,
}

/// Expired / expiring-soon state of a key, for its badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Expired,
    ExpiresSoon,
}

impl Status {
    pub fn text(self) -> &'static str {
        match self {
            Status::Expired => "expired",
            Status::ExpiresSoon => "expires soon",
        }
    }
}

impl Key {
    pub fn status(&self) -> Option<Status> {
        let exp = self.expires?;
        let now = Local::now().timestamp();
        if exp < now {
            return Some(Status::Expired);
        }
        // Python's timedelta.days: whole days, rounded down
        if (exp - now).div_euclid(86_400) <= 30 {
            return Some(Status::ExpiresSoon);
        }
        None
    }

    pub fn expired(&self) -> bool {
        self.expires.is_some_and(|e| e < Local::now().timestamp())
    }
}

/// `YYYY-MM-DD` in local time (what `datetime.fromtimestamp(...).strftime("%Y-%m-%d")` gives).
pub fn ymd(ts: i64) -> String {
    local(ts).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

fn local(ts: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(ts, 0).single()
}

/// Name and email of a user ID ("Name (comment) <email>").
pub fn parse_uid(uid: &str) -> (String, String) {
    let mut email = String::new();
    if let (Some(a), Some(b)) = (uid.find('<'), uid.find('>')) {
        if a < b {
            email = uid[a + 1..b].to_string();
        } else {
            email = String::new();
        }
    }
    let name = uid.split('<').next().unwrap_or("").split('(').next().unwrap_or("").trim().to_string();
    (name, email)
}

fn epoch(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    s.trim().parse::<i64>().ok()
}

/// True only if field 15 holds a real card serial: gpg puts single-character markers there
/// ("#" = offline stub, "+" = secret available locally); a card serial is a long token id.
fn token_serial(f: &[&str]) -> bool {
    f.get(14).is_some_and(|v| {
        let v = v.trim();
        v.chars().count() > 1 && v != "#" && v != "+"
    })
}

/// Parses `--with-colons --fixed-list-mode --list-keys` / `--list-secret-keys` output.
pub fn parse_colons(out: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut cur: Option<Key> = None;
    for line in out.lines() {
        let f: Vec<&str> = line.split(':').collect();
        let rec = f[0];
        match rec {
            "pub" | "sec" => {
                if let Some(k) = cur.take() {
                    keys.push(k);
                }
                let get = |i: usize| f.get(i).copied().unwrap_or("");
                let mut k = Key {
                    keyid: get(4).to_string(),
                    created: epoch(get(5)),
                    expires: epoch(get(6)),
                    secret: rec == "sec",
                    validity: get(1).to_string(),
                    can_encrypt: f.len() <= 11 || get(11).contains('E'),
                    ..Default::default()
                };
                if rec == "sec" && token_serial(&f) {
                    k.on_card = true;
                }
                cur = Some(k);
            }
            "ssb" | "sub" => {
                if let Some(k) = cur.as_mut() {
                    if token_serial(&f) {
                        k.on_card = true;
                    }
                }
            }
            "fpr" => {
                if let Some(k) = cur.as_mut() {
                    if k.fpr.is_empty() {
                        k.fpr = f.get(9).copied().unwrap_or("").to_string();
                    }
                }
            }
            "uid" => {
                if let Some(k) = cur.as_mut() {
                    if f.len() > 9 {
                        k.uids.push(f[9].to_string());
                        if k.name.is_empty() {
                            let (n, e) = parse_uid(f[9]);
                            k.name = n;
                            k.email = e;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(k) = cur {
        keys.push(k);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_parts() {
        assert_eq!(parse_uid("Alice Test <alice@example.com>"), ("Alice Test".into(), "alice@example.com".into()));
        assert_eq!(parse_uid("Bob (work) <b@x.org>"), ("Bob".into(), "b@x.org".into()));
        assert_eq!(parse_uid("macbook"), ("macbook".into(), "".into()));
    }

    #[test]
    fn colons() {
        let out = "sec:u:255:22:FC95631D39E37BAF:1763180000:1826268182::u:::scESC:::+:::23::0:\n\
                   fpr:::::::::E0CBAF1D86225C0294FED047FC95631D39E37BAF:\n\
                   grp:::::::::1234:\n\
                   uid:u::::1763180000::HASH::Alice Test <alice@example.com>::::::::::0:\n\
                   ssb:u:255:18:1111222233334444:1763180000:1826268182:::::e:::D2760001240103040006123456780000:::18:\n\
                   fpr:::::::::AAAA:\n\
                   pub:e:255:22:2A5F2F04F6C87EDA:1704067200:1735603200::u:::sc:::::::23::0:\n\
                   fpr:::::::::6BFB90964DFB5D9E90F8CCE62A5F2F04F6C87EDA:\n\
                   uid:e::::1704067200::HASH::Dave Old <dave@example.com>::::::::::0:\n";
        let k = parse_colons(out);
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].fpr, "E0CBAF1D86225C0294FED047FC95631D39E37BAF");
        assert!(k[0].secret && k[0].on_card && k[0].can_encrypt);
        assert_eq!(k[0].name, "Alice Test");
        assert_eq!(k[1].fpr, "6BFB90964DFB5D9E90F8CCE62A5F2F04F6C87EDA");
        assert!(!k[1].secret && !k[1].on_card && !k[1].can_encrypt);
        assert_eq!(k[1].validity, "e");
        assert_eq!(k[1].expires, Some(1735603200));
    }
}
