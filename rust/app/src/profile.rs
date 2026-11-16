//! Profile & keys: your keys (click one to make it active), the active key's details and
//! actions, generating a key pair, and importing a key.

use egui::{Id, Rect, Sense, Ui, pos2, vec2};
use pgpm_core::gpg;
use pgpm_core::keys::ymd;
use pgpm_core::store;

use crate::app::{App, View};
use crate::panels::{header, kv_row};
use crate::ui::{self, LINE, TextBox, Word, col};

pub struct Profile {
    pub serial: u64,
    pub gen_name: String,
    pub gen_email: String,
    pub gen_pass: String,
    pub generating: bool,
    pub import: TextBox,
    /// the ACTIVE KEY values (selectable)
    pub vals: Vec<String>,
}

impl Profile {
    pub fn new(serial: u64) -> Profile {
        Profile {
            serial,
            gen_name: String::new(),
            gen_email: String::new(),
            gen_pass: String::new(),
            generating: false,
            import: TextBox::new("", false),
            vals: vec![String::new(); 7],
        }
    }
}

/// A flat entry field (34px tall, the entry 14px in).
fn field(ui: &mut Ui, id: Id, x: f32, y: f32, w: f32, value: &mut String, password: bool, live: bool) {
    let r = Rect::from_min_size(pos2(x, y), vec2(w, 34.0));
    ui.painter().rect_filled(r, 0.0, col::INP);
    let er = Rect::from_min_size(pos2(x + 14.0, y + 6.0), vec2(w - 28.0, 22.0));
    let resp = ui.interact(r, id.with("bg"), Sense::click());
    if live && resp.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }
    ui::entry(ui, id, er, value, ui::EntryOpts { password, ..Default::default() }, live);
}

pub fn draw(app: &mut App, p: &mut Profile, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("info", p.serial));
    let (back, _) = header(app, ui, m, id.with("hdr"), "Profile & Keys", true, None);
    if back {
        app.welcome();
        return;
    }
    let area = Rect::from_min_max(pos2(m.min.x, m.min.y + 53.0), m.max);
    let mut act: Option<Action> = None;
    let secret = app.secret.clone();
    let my = app.my_fpr.clone();
    let active = app.active_key().cloned();
    let status = if !app.gpg_ok {
        (col::RD, format!("● GnuPG not found — install {} for key storage", pgpm_core::GPG_DIST.0))
    } else if !secret.is_empty() {
        (col::GN, format!("● {} of your keys in the GnuPG keyring", secret.len()))
    } else if app.card_present == Some(true) {
        (col::T2, "● Keycard detected".to_string())
    } else {
        (col::RD, "● no private keys yet — generate or import one below".to_string())
    };
    ui::scroll_area(ui, id.with("scroll"), area, false, |ui, o| {
        let x0 = o.x + 20.0;
        let cw = area.width() - 40.0;
        let mut y = o.y + 16.0;
        ui::text(ui, x0 + 3.0, y + 3.0, &status.1, status.0);
        y += 25.0;

        // ─ your keys: compact cards flowing left to right; the active one inverted
        if !secret.is_empty() {
            y += 8.0;
            let ctx = ui.ctx().clone();
            let cards: Vec<(String, String, String, f32)> = secret
                .iter()
                .map(|k| {
                    let nm = if k.name.is_empty() { k.keyid.clone() } else { k.name.clone() };
                    let meta = [k.email.as_str(), k.keyid.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("   ");
                    let meta = if k.on_card { format!("smartcard   {meta}") } else { meta };
                    let w = ui::measure(&ctx, &nm).max(ui::measure(&ctx, &meta)).ceil() + 21.0;
                    (k.fpr.clone(), nm, meta, w)
                })
                .collect();
            let maxw = cards.iter().map(|c| c.3).fold(0.0, f32::max);
            let cols = (((cw + 10.0) / (maxw + 10.0)).floor() as usize).max(1);
            let mut colw = vec![0.0f32; cols];
            for (i, c) in cards.iter().enumerate() {
                colw[i % cols] = colw[i % cols].max(c.3);
            }
            let rows = cards.len().div_ceil(cols);
            for (i, (fpr, nm, meta, w)) in cards.iter().enumerate() {
                let cx = x0 + colw[..i % cols].iter().map(|c| c + 10.0).sum::<f32>();
                let cy = y + (i / cols) as f32 * 56.0;
                let r = Rect::from_min_size(pos2(cx, cy), vec2(*w, 50.0));
                let on = Some(fpr) == my.as_ref();
                if on {
                    ui.painter().rect_filled(r, 0.0, col::SEL);
                }
                let resp = ui.interact(r, id.with(("card", i)), if live { Sense::click() } else { Sense::hover() });
                if resp.hovered() && live {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if live && ui::pressed(ui, &resp) {
                    act = Some(Action::Activate(fpr.clone()));
                }
                ui::text_clipped(ui, cx + 10.0, cy + 5.0, nm, if on { col::ON_FILL } else { col::T1 }, r);
                ui::text_clipped(ui, cx + 10.0, cy + 25.0, meta, if on { col::SUB_ON } else { col::T3 }, r);
            }
            y += rows as f32 * 56.0;
        }

        // ─ the active key: details (selectable) and actions
        if let Some(ak) = &active {
            y += 17.0 + 6.0;
            ui::text(ui, x0 + 3.0, y + 3.0, "ACTIVE KEY", col::WT);
            y += 25.0 + 4.0;
            let mut vu = ak.expires.map(ymd).unwrap_or_else(|| "unlimited".into());
            if let Some(st) = ak.status() {
                vu += &format!("  ({})", st.text());
            }
            let vals = [
                ("Name", if ak.name.is_empty() { "—".to_string() } else { ak.name.clone() }),
                ("Email", if ak.email.is_empty() { "—".to_string() } else { ak.email.clone() }),
                ("Key ID", if ak.keyid.is_empty() { "—".to_string() } else { ak.keyid.clone() }),
                ("Fingerprint", if ak.fpr.is_empty() { "—".to_string() } else { ak.fpr.clone() }),
                ("Created", ak.created.map(ymd).unwrap_or_else(|| "—".into())),
                ("Valid until", vu),
                ("Storage", if ak.on_card { "smartcard".into() } else { "this computer".into() }),
            ];
            for (i, (label, v)) in vals.iter().enumerate() {
                p.vals[i] = v.clone();
                y += 2.0;
                kv_row(ui, id.with(("kv", i)), x0, y, cw, label, &mut p.vals[i], None, live);
                y += 27.0;
            }
            y += 10.0;
            let mut links: Vec<(&str, Action)> = vec![("Copy public key", Action::CopyPub)];
            if !ak.on_card {
                links.push(("Copy private key", Action::CopyPriv));
            }
            links.push(("Export public", Action::Export(false)));
            if !ak.on_card {
                links.push(("Export private", Action::Export(true)));
            }
            links.push(("Delete", Action::Delete));
            let words: Vec<Word<'_>> = links.iter().map(|(l, a)| if matches!(a, Action::Delete) { Word::new(l).danger() } else { Word::new(l) }).collect();
            let (hit, h) = ui::link_row(ui, id.with("links"), x0, y, cw, &words, live);
            if let Some(i) = hit {
                act = Some(links[i].1.clone());
            }
            y += h;
        }

        // ─ generate a key pair
        y += 17.0 + 6.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "GENERATE NEW KEY PAIR", col::WT);
        y += 25.0 + 4.0 + 4.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "name", col::T3);
        y += 25.0;
        field(ui, id.with("gname"), x0, y, cw, &mut p.gen_name, false, live);
        y += 34.0 + 6.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "email  (optional)", col::T3);
        y += 25.0;
        field(ui, id.with("gemail"), x0, y, cw, &mut p.gen_email, false, live);
        y += 34.0 + 6.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "password  (optional — protects the key in gpg)", col::T3);
        y += 25.0;
        field(ui, id.with("gpass"), x0, y, cw, &mut p.gen_pass, true, live);
        y += 34.0 + 10.0;
        let label = if p.generating { "Generating…" } else { "Generate  (RSA 4096)" };
        let g = ui::word(ui, id.with("gen"), x0, y, Word::new(label).enabled(!p.generating), live);
        if live && !p.generating && ui::pressed(ui, &g) {
            act = Some(Action::Generate);
        }
        y += LINE;

        // ─ import a key
        y += 17.0 + 6.0;
        ui::text(ui, x0 + 3.0, y + 3.0, "IMPORT KEY  ·  public or private, armored", col::WT);
        y += 25.0 + 4.0 + 4.0 + 2.0;
        ui::text_box(ui, id.with("import"), Rect::from_min_size(pos2(x0, y), vec2(cw, 88.0)), &mut p.import, col::T1, live);
        y += 88.0 + 8.0;
        let (hit, _) = ui::word_row(ui, id.with("ibar"), x0, y, &[Word::new("Import pasted"), Word::new("Import .asc file")], live);
        match hit {
            Some(0) => act = Some(Action::ImportPasted),
            Some(1) => act = Some(Action::ImportAsc),
            _ => {}
        }
        y += LINE + 20.0;
        y - o.y
    });
    if let Some(a) = act {
        run(app, p, a);
    }
}

#[derive(Clone)]
enum Action {
    Activate(String),
    CopyPub,
    CopyPriv,
    Export(bool),
    Delete,
    Generate,
    ImportPasted,
    ImportAsc,
}

fn run(app: &mut App, p: &mut Profile, a: Action) {
    let fpr = app.my_fpr.clone().unwrap_or_default();
    match a {
        Action::Activate(f) => {
            if Some(&f) != app.my_fpr.as_ref() {
                app.set_active_key(Some(f));
            }
        }
        Action::CopyPub => {
            let pubk = gpg::export_pub(&fpr);
            if pubk.is_empty() {
                app.warn("Nothing to copy", "No public key found.");
                return;
            }
            app.ctx.copy_text(pubk);
            app.info("Copied", "Public key copied to clipboard.");
        }
        Action::CopyPriv => match gpg::export_secret(&fpr) {
            Err(e) => app.error("Error", &e),
            Ok(sk) if sk.is_empty() => app.warn("Nothing to copy", "No private key found."),
            Ok(sk) => {
                app.ctx.copy_text(sk);
                app.info("Copied", "Private key copied to clipboard.");
            }
        },
        Action::Export(secret) => export_key(app, &fpr, secret),
        Action::Delete => {
            let nm = app.active_key().map(|k| k.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| fpr.chars().take(16).collect());
            let msg = format!("Delete {nm} (including its private key) from the GnuPG keyring?\nThis cannot be undone.");
            app.ask("Delete key", &msg, move |app, yes| {
                if !yes {
                    return;
                }
                if let Err(e) = gpg::delete_secret(&fpr) {
                    app.error("Error", &e);
                    return;
                }
                app.refresh();
                app.open_info();
            });
        }
        Action::Generate => {
            let name = p.gen_name.trim().to_string();
            if name.is_empty() {
                app.warn("Required", "Name is required.");
                return;
            }
            if !gpg::available() {
                app.error("No GnuPG", &format!("GnuPG is required. Install {}.", pgpm_core::GPG_DIST.0));
                return;
            }
            p.generating = true;
            let email = p.gen_email.trim().to_string();
            let pass = p.gen_pass.clone();
            let serial = p.serial;
            let n2 = name.clone();
            app.spawn(move || gpg::generate(&n2, &email, &pass), move |app, r| match r {
                Err(e) => {
                    app.error("Error", &e);
                    if let View::Info(p) = &mut app.view {
                        if p.serial == serial {
                            p.generating = false;
                        }
                    }
                }
                Ok(_) => {
                    app.reload_keys();
                    if let Some(k) = app.secret.iter().find(|k| k.name == name) {
                        let f = k.fpr.clone();
                        app.set_active_key(Some(f));
                    }
                    app.open_info();
                }
            });
        }
        Action::ImportPasted => {
            let raw = p.import.value().trim().to_string();
            if raw.is_empty() || !raw.contains("BEGIN PGP") {
                app.warn("Required", "Paste an armored PGP key first.");
                return;
            }
            match gpg::import_key(&raw) {
                Err(e) => app.error("Import Error", &e),
                Ok(_) => {
                    app.refresh();
                    app.info("Imported", "Key imported into the GnuPG keyring.");
                    app.open_info();
                }
            }
        }
        Action::ImportAsc => crate::panels::import_asc(app),
    }
}

/// Saves a key as an .asc file.
pub fn export_key(app: &mut App, fpr: &str, secret: bool) {
    let data = if secret {
        match gpg::export_secret(fpr) {
            Ok(d) => d,
            Err(e) => {
                app.error("Export error", &e);
                return;
            }
        }
    } else {
        gpg::export_pub(fpr)
    };
    if data.is_empty() {
        app.warn("Export", "Nothing to export.");
        return;
    }
    let name = if secret { "secret.asc" } else { "public.asc" };
    let Some(out) = rfd::FileDialog::new().set_title("Save key").set_file_name(name).add_filter("ASC files", &["asc"]).save_file() else { return };
    let out = if out.extension().is_none() { out.with_extension("asc") } else { out };
    if let Err(e) = std::fs::write(&out, data.as_bytes()) {
        app.error("Save error", &crate::app::io_error(&e, &out));
        return;
    }
    store::log(&format!("exported {}", out.display()));
    app.info("Exported", "Key saved.");
}
