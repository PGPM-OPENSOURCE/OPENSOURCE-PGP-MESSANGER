//! The smartcard panel (Nitrokey / YubiKey through GnuPG): detect a card, show its details,
//! and the card's commands (PINs, cardholder, key URL, fetch, move a key onto it).

use std::collections::BTreeSet;

use egui::{Id, Rect, Ui, pos2};
use pgpm_core::gpg::{self, CardInfo, Output};
use pgpm_core::store;

use crate::app::{App, View};
use crate::panels::kv_row;
use crate::ui::{self, LINE, Word, col};

pub enum Phase {
    /// just [Detect card] (nothing probed yet)
    Lone,
    Detecting,
    /// probed: no card
    Insert,
    Info(CardInfo),
}

pub struct CardPanel {
    pub serial: u64,
    pub phase: Phase,
    /// "Getting key…" on [Use this card]
    pub busy_use: bool,
    pub vals: Vec<String>,
}

const NOTE: &str = "Use an OpenPGP smartcard through GnuPG. The private key stays on the card; signing and decryption happen on the card (PIN via GnuPG's pinentry dialog).\nOnly Nitrokey has been tested so far; other smartcards may not work at the moment.";

/// This exact card (by serial) is the one "Use this card" was clicked on.
fn in_use(app: &App, info: &CardInfo) -> bool {
    app.card_in_use && info.present && info.get("serial").map(str::to_string) == app.card_serial
}

impl CardPanel {
    /// The panel; a card in use shows its details straight away, then is re-checked quietly.
    pub fn open(app: &mut App) -> CardPanel {
        let serial = app.next_serial();
        let cached = app.card_info.clone();
        let showing = cached.as_ref().is_some_and(|c| app.card_present == Some(true) && in_use(app, c));
        let phase = match cached {
            Some(c) if showing => Phase::Info(c),
            _ => Phase::Lone,
        };
        let p = CardPanel { serial, phase, busy_use: false, vals: Vec::new() };
        if showing {
            detect(app, serial, true);
        }
        p
    }
}

/// Probes the card off the UI thread ("Detecting…" unless `quiet`, which only redraws when
/// the details changed).
fn detect(app: &mut App, serial: u64, quiet: bool) {
    if !quiet {
        if let View::Card(p) = &mut app.view {
            if p.serial == serial {
                p.phase = Phase::Detecting;
            }
        }
    }
    app.spawn(gpg::card_status, move |app, info| {
        let same = app.card_info.as_ref() == Some(&info);
        let View::Card(p) = &mut app.view else { return };
        if p.serial != serial || (quiet && same) {
            return;
        }
        if info.present {
            app.card_info = Some(info.clone());
            p.phase = Phase::Info(info);
        } else {
            p.phase = Phase::Insert;
        }
    });
}

pub fn draw(app: &mut App, p: &mut CardPanel, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("card", p.serial));
    let x0 = m.min.x + 20.0;
    if !app.gpg_ok {
        let mut y = m.min.y + 18.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "● GnuPG is required for hardware keycards", col::RD);
        y += 25.0 + 4.0;
        ui::text(ui, x0 + 3.0, y + 3.0, &format!("Install {}, then plug in your Nitrokey or YubiKey.", pgpm_core::GPG_DIST.0), col::T2);
        y += 25.0 + 2.0;
        ui::text(ui, x0 + 3.0, y + 3.0, pgpm_core::GPG_DIST.1, col::T3);
        return;
    }
    // the note, pinned to the bottom
    let lines = ui::wrap(ui.ctx(), NOTE, 640.0);
    let nh = lines.len() as f32 * LINE + 6.0;
    let ny = m.max.y - 14.0 - nh;
    ui::lines(ui, x0 + 3.0, ny + 3.0, &lines, col::T3);
    let area = Rect::from_min_max(m.min, pos2(m.max.x, ny - 8.0));
    // [Detect card] wherever it is shown; the grid's commands
    let mut cmd: Option<&'static str> = None;
    let cw = m.width() - 40.0;
    let phase_info = match &p.phase {
        Phase::Info(i) => Some(i.clone()),
        _ => None,
    };
    let using = phase_info.as_ref().is_some_and(|i| in_use(app, i));
    ui::scroll_area(ui, id.with("scroll"), area, false, |ui, o| {
        let x = o.x + 20.0;
        let mut y = o.y + 18.0;
        match &p.phase {
            Phase::Lone | Phase::Detecting | Phase::Insert => {
                let msg = match p.phase {
                    Phase::Detecting => Some("Detecting…"),
                    Phase::Insert => Some("Insert your smartcard"),
                    _ => None,
                };
                if let Some(t) = msg {
                    ui::text(ui, x + 3.0, y + 3.0, t, col::T1);
                    y += 25.0 + 19.0;
                }
                if !matches!(p.phase, Phase::Detecting) {
                    let r = ui::word(ui, id.with("detect"), x, y, Word::new("Detect card"), live);
                    if live && ui::pressed(ui, &r) {
                        cmd = Some("detect");
                    }
                    y += LINE;
                }
            }
            Phase::Info(info) => {
                let fields = [
                    ("Cardholder", "name"),
                    ("Serial", "serial"),
                    ("Version", "version"),
                    ("Reader", "reader"),
                    ("Signature key", "sig_fpr"),
                    ("Encryption key", "enc_fpr"),
                    ("Auth key", "auth_fpr"),
                    ("PIN retries", "pin_retries"),
                ];
                let shown: Vec<(&str, String)> = fields.iter().filter_map(|(l, k)| info.get(k).map(|v| (*l, v.to_string()))).collect();
                if p.vals.len() != shown.len() {
                    p.vals = shown.iter().map(|(_, v)| v.clone()).collect();
                }
                for (i, (label, v)) in shown.iter().enumerate() {
                    p.vals[i] = v.clone();
                    y += 2.0;
                    kv_row(ui, id.with(("kv", i)), x, y, cw, label, &mut p.vals[i], None, live);
                    y += 27.0;
                }
                // the commands on an even grid (Detect always last); a card in use has no
                // [Use this card]
                let mut items: Vec<(&'static str, &'static str)> = Vec::new();
                if !using {
                    items.push((if p.busy_use { "Getting key…" } else { "Use this card" }, "use"));
                }
                items.extend([
                    ("Move key to card", "move"),
                    ("Change PIN", "pin1"),
                    ("Admin PIN", "pin3"),
                    ("Unblock PIN", "pin2"),
                    ("Set name", "name"),
                    ("Set key URL", "url"),
                    ("Fetch key", "fetch"),
                    (if using { "Detect another card" } else { "Detect card" }, "detect"),
                ]);
                let cols = grid_cols(ui.ctx(), cw, &items);
                for (ri, chunk) in items.chunks(cols).enumerate() {
                    y += 8.0;
                    let mut xx = x;
                    for (ci, (label, c)) in chunk.iter().enumerate() {
                        let enabled = !(*c == "use" && p.busy_use);
                        let r = ui::word(ui, id.with(("cmd", ri, ci)), xx, y, Word::new(label).enabled(enabled), live);
                        if live && enabled && ui::pressed(ui, &r) {
                            cmd = Some(c);
                        }
                        xx = r.rect.max.x + 8.0;
                    }
                    y += LINE;
                }
            }
        }
        y + 16.0 - o.y
    });
    let Some(c) = cmd else { return };
    let serial = p.serial;
    let info = phase_info.unwrap_or_default();
    let enc = info.get("enc_fpr").or(info.get("sig_fpr")).map(str::to_string);
    match c {
        "detect" => {
            p.phase = Phase::Detecting;
            detect_into(app, p, serial);
        }
        "use" => use_card(app, p, info, enc),
        "move" => {
            if !app.secret.iter().any(|k| !k.on_card) {
                app.warn("No key", "No software secret key to move. Generate or import one first.");
                return;
            }
            crate::panels::choose_secret(app, "Move which key to the card?");
        }
        "pin1" => card_action(app, || gpg::card_change_pin("1"), "PIN updated.", None),
        "pin3" => card_action(app, || gpg::card_change_pin("3"), "Admin PIN updated.", None),
        "pin2" => card_action(app, || gpg::card_change_pin("2"), "PIN unblocked.", None),
        "name" => card_name(app),
        "url" => card_url(app, info),
        "fetch" => {
            let check: Check = Box::new(move |out, _| fetch_problem(out, enc.as_deref()));
            card_action(app, gpg::card_fetch, "Fetched public key from URL.", Some(check));
        }
        _ => {}
    }
}

/// [Detect card] pressed while the panel itself is borrowed for drawing.
fn detect_into(app: &mut App, _p: &mut CardPanel, serial: u64) {
    app.spawn(gpg::card_status, move |app, info| {
        let View::Card(p) = &mut app.view else { return };
        if p.serial != serial {
            return;
        }
        if info.present {
            app.card_info = Some(info.clone());
            p.phase = Phase::Info(info);
        } else {
            p.phase = Phase::Insert;
        }
    });
}

/// A full, even grid: the column count (3, 4 or 2) that fills every row while the longest
/// label keeps room; else 3.
fn grid_cols(ctx: &egui::Context, cw: f32, items: &[(&str, &str)]) -> usize {
    let need = items.iter().map(|(l, _)| ui::measure(ctx, l).ceil()).fold(0.0, f32::max) + 32.0;
    for c in [3usize, 4, 2] {
        let w = ((cw - (c as f32 - 1.0) * 8.0) / c as f32).floor();
        if items.len() % c == 0 && w >= need {
            return c;
        }
    }
    3
}

type Check = Box<dyn FnOnce(&str, Option<&CardInfo>) -> Option<String> + Send>;

/// Runs a card command (which may pop gpg's pinentry) off the UI thread, then reports and
/// shows the panel again. `check(output, card after)` may say why it didn't take effect.
pub fn card_action(app: &mut App, work: impl FnOnce() -> Result<Output, String> + Send + 'static, ok_msg: &str, check: Option<Check>) {
    card_action_with(app, move || (work(), None), ok_msg, check)
}

fn card_action_with(
    app: &mut App,
    work: impl FnOnce() -> (Result<Output, String>, Option<CardInfo>) + Send + 'static,
    ok_msg: &str,
    check: Option<Check>,
) {
    let ok_msg = ok_msg.to_string();
    app.spawn(work, move |app, (r, after)| {
        match r {
            Err(e) => app.error("Card error", &e),
            Ok(r) => {
                let rc = r.code.unwrap_or(-1);
                let out = format!("{}{}", r.stderr, r.stdout).trim().to_string();
                let low = out.to_lowercase();
                if rc != 0 && (low.contains("error") || low.contains("failed")) {
                    let msg: String = out.chars().take(600).collect();
                    app.error("Card error", if msg.is_empty() { "command failed" } else { &msg });
                } else if let Some(why) = check.and_then(|c| c(&out, after.as_ref())) {
                    app.warn("Not done", &why);
                } else {
                    app.info("Done", &ok_msg);
                }
                app.reload_keys();
            }
        }
        app.open_card();
    });
}

/// Why the card's public key still isn't in the keyring after a fetch (gpg exits 0 even when
/// it skipped the key), or None if it arrived.
fn fetch_problem(out: &str, fpr: Option<&str>) -> Option<String> {
    let fpr = fpr?;
    if !gpg::export_pub(fpr).is_empty() {
        return None;
    }
    if out.contains("no user ID") {
        return Some(
            "The key at the card's URL has no name or email (user ID), so GnuPG skipped it. \
             keys.openpgp.org leaves those out until you confirm the email address there.\n\n\
             Confirm it on keys.openpgp.org, set a key URL that serves the whole key (e.g. from \
             keyserver.ubuntu.com), or import your public key's .asc file."
                .into(),
        );
    }
    let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("gpg: ")).collect();
    let tail = lines[lines.len().saturating_sub(4)..].join("\n");
    Some(format!("The card's public key wasn't imported.\n\n{}", if tail.is_empty() { "Set a key URL on the card first." } else { &tail }))
}

/// [Use this card]: the card's key becomes the active one (getting its public key first).
fn use_card(app: &mut App, p: &mut CardPanel, info: CardInfo, fpr: Option<String>) {
    let Some(fpr) = fpr else {
        app.warn("No key", "Card has no key.");
        return;
    };
    if gpg::export_pub(&fpr).is_empty() {
        card_pubkey(app, p, info, fpr);
        return;
    }
    app.card_present = Some(true);
    app.card_in_use = true;
    app.card_serial = info.get("serial").map(str::to_string);
    app.reload_keys();
    let primary = gpg::primary_fpr(&fpr);
    app.set_active_key(Some(primary));
    app.info("Keycard active", "This card's key is now your active signing/decryption key.");
    app.open_card();
}

fn set_busy(app: &mut App, serial: u64, on: bool) {
    if let View::Card(p) = &mut app.view {
        if p.serial == serial {
            p.busy_use = on;
        }
    }
}

/// "Use this card" while GnuPG lacks the card's public key: fetch it from the card's key URL,
/// else offer to look it up on keyserver.ubuntu.com; use the card once it's in.
fn card_pubkey(app: &mut App, p: &mut CardPanel, info: CardInfo, fpr: String) {
    let serial = p.serial;
    if info.get("url").is_some() {
        p.busy_use = true;
        let (i2, f2) = (info.clone(), fpr.clone());
        app.spawn(
            || {
                let r = gpg::card_fetch();
                gpg::card_status();
                r
            },
            move |app, r| fetched(app, serial, i2, f2, gpg_out(&r)),
        );
    } else {
        fetched(app, serial, info, fpr, String::new());
    }
}

fn gpg_out(r: &Result<Output, String>) -> String {
    match r {
        Err(e) => e.clone(),
        Ok(o) => format!("{}{}", o.stderr, o.stdout),
    }
}

fn retry(app: &mut App, serial: u64, info: CardInfo, fpr: String) {
    // use the card now that its public key is in (as if pressed again)
    let mut tmp = CardPanel { serial, phase: Phase::Lone, busy_use: false, vals: Vec::new() };
    use_card(app, &mut tmp, info, Some(fpr));
}

fn fetched(app: &mut App, serial: u64, info: CardInfo, fpr: String, out: String) {
    set_busy(app, serial, false);
    if !gpg::export_pub(&fpr).is_empty() {
        retry(app, serial, info, fpr);
        return;
    }
    let why = if out.contains("no user ID") {
        "The card's key URL gives the key without a name or email (keys.openpgp.org leaves those out until the address is confirmed there), so GnuPG can't import it."
    } else if info.get("url").is_some() {
        "Fetching it from the card's key URL didn't work."
    } else {
        "The card has no key URL to fetch it from."
    };
    let msg = format!("{why}\n\nLook it up on keyserver.ubuntu.com instead? You can also import its .asc file in Profile & Keys.");
    app.dialog(
        crate::dialog::Dialog::new("Public key not in keyring", &msg, col::AM)
            .buttons(&[("Cancel", 0), ("Look up", 1)])
            .then(move |app, v, _| {
                if v != 1 {
                    return;
                }
                set_busy(app, serial, true);
                let key = info.get("sig_fpr").map(str::to_string).unwrap_or_else(|| fpr.clone());
                let (i2, f2) = (info.clone(), fpr.clone());
                app.spawn(
                    move || {
                        let r = gpg::recv_key(&key);
                        gpg::card_status();
                        r
                    },
                    move |app, r| {
                        set_busy(app, serial, false);
                        if !gpg::export_pub(&f2).is_empty() {
                            retry(app, serial, i2, f2);
                            return;
                        }
                        let out = gpg_out(&r);
                        let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("gpg: ")).collect();
                        let tail: String = lines[lines.len().saturating_sub(3)..].iter().map(|l| format!("{l}\n")).collect();
                        app.warn(
                            "Key not found",
                            &format!("keyserver.ubuntu.com didn't have it either.\n\n{tail}\nImport your public key's .asc file in Profile & Keys."),
                        );
                    },
                );
            }),
    );
}

/// [Set name]: the cardholder's surname and given name.
fn card_name(app: &mut App) {
    app.ask_string("Cardholder", "Surname:", "", |app, sn| {
        let Some(sn) = sn else { return };
        app.ask_string("Cardholder", "Given name:", "", move |app, gn| {
            let Some(gn) = gn else { return };
            card_action(app, move || gpg::card_set_name(&sn, &gn), "Cardholder name set.", None);
        });
    });
}

/// [Set key URL]: the address is checked first (the card stores any text), then written and
/// read back from the card.
fn card_url(app: &mut App, info: CardInfo) {
    app.ask_string("Public-key URL", "URL where your public key is published:", "", move |app, url| {
        let url = url.unwrap_or_default().trim().to_string();
        if url.is_empty() {
            return;
        }
        let fprs: BTreeSet<String> = [info.get("sig_fpr"), info.get("enc_fpr")].into_iter().flatten().map(str::to_string).collect();
        let u2 = url.clone();
        app.spawn(move || pgpm_core::net::key_url_problem(&u2, &fprs), move |app, problem| {
            let go = move |app: &mut App| {
                let u3 = url.clone();
                let check: Check = Box::new(move |_out, after| {
                    let now = after.and_then(|a| a.get("url")).unwrap_or("");
                    if now == u3 { None } else { Some(format!("The card's key URL is still:\n{}", if now.is_empty() { "(none)" } else { now })) }
                });
                let u4 = url.clone();
                card_action_with(
                    app,
                    move || {
                        let r = gpg::card_set_url(&u4);
                        let after = gpg::card_status();
                        (r, Some(after))
                    },
                    &format!("The card's key URL is now:\n{url}"),
                    Some(check),
                );
            };
            match problem {
                Some(p) => app.ask("Check the key URL", &format!("{p}\n\nSet it on the card anyway?"), move |app, yes| {
                    if yes {
                        go(app);
                    }
                }),
                None => go(app),
            }
        });
    });
    store::log("card_url: asked");
}
