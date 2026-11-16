//! The panels right of the sidebar: welcome, a contact's chat box, Sign / Encrypt,
//! Decrypt / Verify, Add contact, a key's details, and the pick-a-key list. (The smartcard
//! and Profile & keys panels have their own modules.)

use egui::{Color32, Id, Rect, Sense, Ui, pos2, vec2};
use pgpm_core::gpg;
use pgpm_core::keys::{Key, ymd};
use pgpm_core::store;

use crate::app::{App, View};
use crate::ui::{self, GAP, LINE, TK_CH, TextBox, Word, col};

pub fn draw(app: &mut App, view: &mut View, ui: &mut Ui, m: Rect) {
    match view {
        View::Welcome => {
            ui::text(ui, m.min.x + 19.0, m.min.y + 17.0, &format!("PGPM v{}", pgpm_core::APP_VERSION), col::T3);
        }
        View::Chat(c) => chat(app, c, ui, m),
        View::Encrypt(e) => encrypt(app, e, ui, m),
        View::Decrypt(d) => decrypt(app, d, ui, m),
        View::Add(a) => add(app, a, ui, m),
        View::Key(k) => key_view(app, k, ui, m),
        View::Choose(c) => choose(app, c, ui, m),
        View::Card(c) => crate::card::draw(app, c, ui, m),
        View::Info(p) => crate::profile::draw(app, p, ui, m),
    }
}

/// The message box of the box panels (top in line with the search field, the button row
/// at the bottom in line with the sidebar's last tab).
fn box_rect(m: Rect) -> Rect {
    Rect::from_min_max(pos2(m.min.x + 16.0, m.min.y + 11.0), pos2(m.max.x - 16.0, m.max.y - 12.0 - LINE - 10.0))
}

fn bar_y(m: Rect) -> f32 {
    m.max.y - 12.0 - LINE
}

/// The header of a panel: [Back], a title, words on the right. Returns (back pressed, right
/// word pressed).
pub fn header(app: &App, ui: &mut Ui, m: Rect, id: Id, title: &str, back: bool, right: Option<Word<'_>>) -> (bool, bool) {
    let live = app.dialogs.is_empty();
    let mut x = m.min.x;
    let mut b = false;
    if back {
        let r = ui::word(ui, id.with("back"), m.min.x + 14.0, m.min.y + 16.0, Word::new("Back"), live);
        b = live && ui::pressed(ui, &r);
        x = r.rect.max.x;
    }
    if !title.is_empty() {
        let pad = if back { 8.0 } else { 14.0 };
        ui::text(ui, x + 2.0 + pad, m.min.y + 16.0, title, col::WT);
    }
    let mut rp = false;
    if let Some(w) = right {
        let ww = ui::word_w(ui.ctx(), w.text);
        let r = ui::word(ui, id.with("right"), m.max.x - 16.0 - ww, m.min.y + 16.0, w, live);
        rp = live && ui::pressed(ui, &r);
    }
    (b, rp)
}

fn copy(ui: &Ui, t: &str) {
    ui.ctx().copy_text(t.to_string());
}

fn pick_file(title: &str, asc_first: bool) -> Option<std::path::PathBuf> {
    let d = rfd::FileDialog::new().set_title(title);
    let d = if asc_first {
        d.add_filter("PGP / ASC files", &["asc", "gpg", "pgp", "txt"])
    } else {
        d
    };
    d.pick_file()
}

// -------------------------------------------------------------------------------------------
// A contact's chat box
// -------------------------------------------------------------------------------------------
pub struct Chat {
    pub fpr: String,
    pub text: TextBox,
    pub field: Option<Rect>,
    pub serial: u64,
}

impl Chat {
    pub fn new(fpr: &str, name: &str, serial: u64) -> Chat {
        Chat { fpr: fpr.into(), text: TextBox::new(&format!("Type a message to {name}…"), true), field: None, serial }
    }
}

fn chat(app: &mut App, c: &mut Chat, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("chat", c.serial));
    let br = box_rect(m);
    c.field = Some(br);
    ui::text_box(ui, id.with("text"), br, &mut c.text, col::T1, live);
    let words = [Word::new("Encrypt"), Word::new("Decrypt"), Word::new("Copy")];
    let (hit, _) = ui::word_row(ui, id.with("bar"), m.min.x + 16.0, bar_y(m), &words, live);
    // the chat's text, without the placeholder
    let text = c.text.value().trim().to_string();
    match hit {
        Some(0) => {
            if text.is_empty() {
                return;
            }
            let Some(idx) = app.contact_index(&c.fpr) else { return };
            let fpr = app.contacts[idx].fpr.clone();
            if !app.has_key() {
                app.no_key_warning("encrypt (so you can read your own messages)");
                return;
            }
            let mut recips = vec![fpr];
            if let Some(me) = app.my_fpr.clone() {
                if !recips.contains(&me) {
                    recips.push(me);
                }
            }
            if let Some(why) = app.cant_encrypt(&recips) {
                app.warn("Can't encrypt", &why);
                return;
            }
            match gpg::encrypt(&text, &recips, None) {
                Ok(out) => c.text.set(out),
                Err(e) => app.error("Encryption Error", &e),
            }
        }
        Some(1) => {
            if !app.has_key() {
                app.no_key_warning("decrypt");
                return;
            }
            if text.is_empty() {
                return;
            }
            if !text.contains("-----BEGIN PGP MESSAGE-----") {
                app.warn("Not PGP", "Paste a PGP encrypted message block to decrypt.");
                return;
            }
            let serial = c.serial;
            app.spawn(move || gpg::decrypt(&text), move |app, r| {
                let View::Chat(c) = &mut app.view else { return };
                if c.serial != serial {
                    return;
                }
                match r {
                    Ok((plain, _)) => c.text.set(plain),
                    Err(e) => app.error("Decryption Error", &e),
                }
            });
        }
        Some(2) => {
            if !text.is_empty() {
                copy(ui, &text);
            }
        }
        _ => {}
    }
}

// -------------------------------------------------------------------------------------------
// Sign / Encrypt
// -------------------------------------------------------------------------------------------
pub struct Encrypt {
    pub text: TextBox,
    /// label -> fingerprint (the pick list's choices)
    pub opts: Vec<(String, String)>,
    pub labels: Vec<String>,
    pub pick: String,
    pub pop: bool,
    pub field: Option<Rect>,
    pub serial: u64,
}

impl Encrypt {
    pub fn new(app: &App, serial: u64) -> Encrypt {
        let opts = app.recipients();
        let mut labels: Vec<String> = opts.iter().map(|(l, _)| l.clone()).collect();
        if labels.is_empty() {
            labels.push("(no keys)".into());
        }
        Encrypt {
            text: TextBox::new("Type or load a message…", false),
            pick: labels[0].clone(),
            opts,
            labels,
            pop: false,
            field: None,
            serial,
        }
    }
}

fn encrypt(app: &mut App, e: &mut Encrypt, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("encrypt", e.serial));
    let br = box_rect(m);
    e.field = Some(br);
    ui::text_box(ui, id.with("text"), br, &mut e.text, col::T1, live);
    let y = bar_y(m);
    let x0 = m.min.x + 16.0;
    // [Encrypt][▾]: the ▾ opens the pick list of who to encrypt to
    let enc = ui::word(ui, id.with("enc"), x0, y, Word::new("Encrypt"), live);
    let tip = if e.opts.iter().any(|(l, _)| *l == e.pick) { format!("Encrypt to {}", e.pick) } else { "Import a key first".into() };
    ui::tip(ui, &enc, &tip);
    let chev = ui::word(ui, id.with("chev"), enc.rect.max.x, y, Word::new("▾"), live);
    ui::tip(ui, &chev, "Pick who to encrypt to");
    let split = Rect::from_min_max(enc.rect.min, chev.rect.max);
    let rest = [Word::new("Import file"), Word::new("Sign"), Word::new("Save"), Word::new("Copy")];
    let (hit, _) = ui::word_row(ui, id.with("bar"), split.max.x + GAP, y, &rest, live);
    let mut closed_now = false;
    if live && ui::pressed(ui, &chev) {
        e.pop = !e.pop;
        closed_now = true;
    }
    if e.pop && !closed_now {
        pick_list(ui, id.with("pop"), split, e);
    } else if e.pop {
        pick_list(ui, id.with("pop"), split, e);
    }
    let body = e.text.value().to_string();
    if live && ui::pressed(ui, &enc) {
        e.pop = false;
        do_encrypt(app, e, &body);
    }
    match hit {
        Some(0) => {
            e.pop = false;
            if let Some(p) = pick_file("Import file", false) {
                load_into(app, &mut e.text, &p);
            }
        }
        Some(1) => {
            e.pop = false;
            if !app.has_key() {
                app.no_key_warning("sign");
                app.open_key_setup();
                return;
            }
            if body.trim().is_empty() {
                return;
            }
            let fpr = app.my_fpr.clone();
            let serial = e.serial;
            app.spawn(move || gpg::sign(&body, fpr.as_deref()), move |app, r| {
                let View::Encrypt(e) = &mut app.view else { return };
                if e.serial != serial {
                    return;
                }
                match r {
                    Ok(out) => e.text.set(out),
                    Err(err) => app.error("Sign Error", &err),
                }
            });
        }
        Some(2) => {
            e.pop = false;
            if body.trim().is_empty() {
                return;
            }
            if let Some(p) = rfd::FileDialog::new().set_title("Save").add_filter("ASC files", &["asc"]).save_file() {
                let p = if p.extension().is_none() { p.with_extension("asc") } else { p };
                match std::fs::write(&p, body.as_bytes()) {
                    Ok(_) => app.info("Saved", "File saved."),
                    Err(err) => app.error("Save Error", &crate::app::io_error(&err, &p)),
                }
            }
        }
        Some(3) => {
            e.pop = false;
            if !body.trim().is_empty() {
                copy(ui, &body);
            }
        }
        _ => {}
    }
}

fn do_encrypt(app: &mut App, e: &mut Encrypt, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    let Some(fpr) = e.opts.iter().find(|(l, _)| *l == e.pick).map(|(_, f)| f.clone()) else {
        app.warn("No Recipient", "Pick a recipient (import a key first).");
        return;
    };
    if !app.has_key() {
        app.no_key_warning("encrypt (so you can read your own messages)");
        return;
    }
    let mut recips = vec![fpr];
    if let Some(me) = app.my_fpr.clone() {
        if !recips.contains(&me) {
            recips.push(me);
        }
    }
    if let Some(why) = app.cant_encrypt(&recips) {
        app.warn("Can't encrypt", &why);
        return;
    }
    match gpg::encrypt(body, &recips, None) {
        Ok(out) => e.text.set(out),
        Err(err) => app.error("Encrypt Error", &err),
    }
}

/// The console pick list over the text box: the pick marked ">", the row under the pointer
/// inverted; a release picks, a press elsewhere (or Escape) closes it.
fn pick_list(ui: &mut Ui, id: Id, split: Rect, e: &mut Encrypt) {
    let ctx = ui.ctx().clone();
    let n = e.labels.len().clamp(1, 8);
    let maxlen = e.labels.iter().map(|l| l.chars().count()).max().unwrap_or(4);
    let pw = split.width().max((maxlen + 4) as f32 * TK_CH);
    let ph = n as f32 * 20.0 + 2.0;
    let mut y = split.min.y - ph;
    if y < 4.0 {
        y = split.max.y;
    }
    let r = Rect::from_min_size(pos2(split.min.x, y), vec2(pw, ph));
    let layer = egui::LayerId::new(egui::Order::Foreground, id);
    let p = ctx.layer_painter(layer);
    p.rect_filled(r, 0.0, col::POP);
    let ptr = ctx.input(|i| i.pointer.latest_pos());
    let mut picked = None;
    for (i, l) in e.labels.iter().take(n).enumerate() {
        let row = Rect::from_min_size(pos2(r.min.x + 1.0, r.min.y + 1.0 + i as f32 * 20.0), vec2(pw - 2.0, 20.0));
        let hot = ptr.is_some_and(|q| row.contains(q));
        if hot {
            p.rect_filled(row, 0.0, col::SEL);
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let t = format!("{}{}", if *l == e.pick { "> " } else { "  " }, l);
        let c = if hot { col::ON_FILL } else { col::T1 };
        let g = ui::galley(&ctx, &t, c);
        p.with_clip_rect(row).galley(pos2(row.min.x, ui::galley_y(&g, row.min.y - 1.0)), g, c);
        if hot && ctx.input(|i| i.pointer.primary_released()) {
            picked = Some(l.clone());
        }
    }
    // the list takes the pointer: nothing under it reacts
    let blocker = egui::Area::new(id.with("area")).order(egui::Order::Foreground).fixed_pos(r.min);
    blocker.show(&ctx, |ui| {
        ui.allocate_rect(Rect::from_min_size(r.min, r.size()), Sense::click());
    });
    if let Some(l) = picked {
        e.pick = l;
        e.pop = false;
        return;
    }
    let outside_press = ctx.input(|i| i.pointer.primary_pressed()) && ptr.is_some_and(|q| !r.contains(q) && !split.contains(q));
    if outside_press || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        e.pop = false;
    }
}

fn load_into(app: &mut App, tb: &mut TextBox, p: &std::path::Path) {
    match std::fs::read(p) {
        Ok(b) => tb.set(String::from_utf8_lossy(&b).into_owned()),
        Err(err) => app.error("Read Error", &crate::app::io_error(&err, p)),
    }
}

// -------------------------------------------------------------------------------------------
// Decrypt / Verify
// -------------------------------------------------------------------------------------------
pub struct Decrypt {
    pub text: TextBox,
    /// (text, colour, shown until)
    pub status: Option<(String, Color32, f64)>,
    pub field: Option<Rect>,
    pub serial: u64,
}

impl Decrypt {
    pub fn new(serial: u64) -> Decrypt {
        Decrypt { text: TextBox::new("Paste or load an encrypted / signed message…", false), status: None, field: None, serial }
    }
}

/// `signer` shortened so " valid signature from <signer>" fits `room` (20px kept for the
/// check mark): the <email> goes first, then the name is cut with "…".
fn fit_signer(ctx: &egui::Context, room: f32, signer: &str) -> String {
    let prefix = " valid signature from ";
    let room = room - 20.0;
    if room <= 0.0 {
        return signer.into();
    }
    let name = signer.split('<').next().unwrap_or("").trim();
    let name = if name.is_empty() { signer } else { name };
    for cand in [signer, name] {
        if ui::measure(ctx, &format!("{prefix}{cand}")) <= room {
            return cand.into();
        }
    }
    let mut n: Vec<char> = name.chars().collect();
    while n.len() > 1 && ui::measure(ctx, &format!("{prefix}{}…", n.iter().collect::<String>())) > room {
        n.pop();
    }
    format!("{}…", n.iter().collect::<String>().trim_end())
}

fn decrypt(app: &mut App, d: &mut Decrypt, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("decrypt", d.serial));
    let br = box_rect(m);
    d.field = Some(br);
    ui::text_box(ui, id.with("text"), br, &mut d.text, col::T1, live);
    let words = [Word::new("Decrypt / Verify"), Word::new("Import file"), Word::new("Copy")];
    let y = bar_y(m);
    let (hit, end) = ui::word_row(ui, id.with("bar"), m.min.x + 16.0, y, &words, live);
    // the status line takes the rest of the row
    let now = ui.input(|i| i.time);
    let sw = (m.max.x - 16.0 - end).max(0.0);
    if let Some((t, c, until)) = &d.status {
        if now >= *until {
            d.status = None;
        } else {
            let clip = Rect::from_min_size(pos2(end, y), vec2(sw, LINE));
            ui::text_clipped(ui, end, y, t, *c, clip);
            if let Ok(d) = std::time::Duration::try_from_secs_f64(until - now) {
                ui.ctx().request_repaint_after(d);
            }
        }
    }
    match hit {
        Some(0) => {
            let raw = d.text.value().trim().to_string();
            if raw.is_empty() {
                return;
            }
            if !raw.contains("BEGIN PGP") {
                app.error("Not PGP", "Paste a PGP encrypted or signed message.");
                return;
            }
            if raw.contains("BEGIN PGP MESSAGE") && !app.has_key() {
                app.no_key_warning("decrypt");
                app.open_key_setup();
                return;
            }
            let serial = d.serial;
            app.spawn(move || gpg::decrypt(&raw), move |app, r| {
                let ctx = app.ctx.clone();
                let View::Decrypt(d) = &mut app.view else { return };
                if d.serial != serial {
                    return;
                }
                match r {
                    Ok((text, signer)) => {
                        d.text.set(text);
                        let until = ctx.input(|i| i.time) + 4.0;
                        d.status = Some(match signer {
                            Some(s) => {
                                let s = fit_signer(&ctx, sw, &s);
                                (format!("✓ valid signature from {s}"), col::GN, until)
                            }
                            None => ("• processed (no verifiable signature)".into(), col::T3, until),
                        });
                    }
                    Err(e) => app.error("Decrypt Error", &e),
                }
            });
        }
        Some(1) => {
            if let Some(p) = pick_file("Import file", false) {
                load_into(app, &mut d.text, &p);
            }
        }
        Some(2) => {
            let t = d.text.value().to_string();
            if !t.trim().is_empty() {
                copy(ui, &t);
            }
        }
        _ => {}
    }
}

// -------------------------------------------------------------------------------------------
// Add contact
// -------------------------------------------------------------------------------------------
pub struct Add {
    pub text: TextBox,
    pub serial: u64,
}

impl Add {
    pub fn new(serial: u64) -> Add {
        Add { text: TextBox::new("paste pgp key here…", true), serial }
    }
}

fn add(app: &mut App, a: &mut Add, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("add", a.serial));
    ui::text_box(ui, id.with("text"), box_rect(m), &mut a.text, col::T1, live);
    let words = [Word::new("Import"), Word::new("Import .asc file"), Word::new("Cancel")];
    let (hit, _) = ui::word_row(ui, id.with("bar"), m.min.x + 16.0, bar_y(m), &words, live);
    match hit {
        Some(0) => {
            let raw = a.text.value().trim().to_string();
            store::log(&format!("save_contact: raw_len={} has_begin={}", raw.len(), if raw.contains("BEGIN PGP") { "True" } else { "False" }));
            if raw.is_empty() || !raw.contains("BEGIN PGP") {
                app.warn("Required", "Paste an armored PGP key.");
                return;
            }
            match gpg::import_key(&raw) {
                Ok(_) => {
                    app.refresh();
                    app.info("Imported", "Key imported into the keyring.");
                    app.welcome();
                }
                Err(e) => app.error("Import error", &e),
            }
        }
        Some(1) => import_asc(app),
        Some(2) => app.welcome(),
        _ => {}
    }
}

/// Imports a key from an .asc file (Add contact, Profile & keys).
pub fn import_asc(app: &mut App) {
    let Some(p) = pick_file("Import key (.asc)", true) else { return };
    let raw = match std::fs::read(&p) {
        Ok(b) => String::from_utf8_lossy(&b).trim().to_string(),
        Err(e) => {
            app.error("Read Error", &crate::app::io_error(&e, &p));
            return;
        }
    };
    if raw.is_empty() || !raw.contains("BEGIN PGP") {
        app.error("Not a Key", "That file doesn't contain a PGP key block.");
        return;
    }
    match gpg::import_key(&raw) {
        Ok(_) => {
            app.refresh();
            app.info("Imported", "Key imported into the GnuPG keyring.");
            app.welcome();
        }
        Err(e) => app.error("Import error", &e),
    }
}

// -------------------------------------------------------------------------------------------
// A key's details (the contact's [i])
// -------------------------------------------------------------------------------------------
pub struct KeyView {
    pub key: Key,
    /// the contact highlighted when it opened ([Back] returns to its chat)
    pub back: Option<usize>,
    pub pubkey: TextBox,
    pub vals: Vec<String>,
    pub serial: u64,
}

impl KeyView {
    pub fn new(key: Key, back: Option<usize>, serial: u64) -> KeyView {
        let pk = gpg::export_pub(&key.fpr);
        let mut pubkey = TextBox::new("", false);
        pubkey.set(if pk.is_empty() { "(no key)".to_string() } else { pk });
        KeyView { key, back, pubkey, vals: Vec::new(), serial }
    }
}

/// A "Label:  value" row (the value selectable and copyable); `entry_w` hugs the value when
/// given (in Tk characters). Returns the value's rect.
pub fn kv_row(ui: &mut Ui, id: Id, x: f32, y: f32, w: f32, label: &str, value: &mut String, entry_w: Option<f32>, live: bool) -> Rect {
    ui::text(ui, x + 3.0, y + 3.0, &format!("{label}:"), col::T3);
    let lw = 16.0 * TK_CH + 6.0;
    let ew = entry_w.unwrap_or(w - lw);
    let r = Rect::from_min_size(pos2(x + lw, y + 2.0), vec2(ew, 21.0));
    ui::entry(ui, id, r, value, ui::EntryOpts { readonly: true, ..Default::default() }, live);
    r
}

fn key_view(app: &mut App, k: &mut KeyView, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("key", k.serial));
    let (back, del) = header(app, ui, m, id.with("hdr"), "", true, Some(Word::new("Delete").danger()));
    let c = k.key.clone();
    let fx = m.min.x + 24.0;
    let fw = m.width() - 48.0;
    let mut y = m.min.y + 53.0 + 12.0;
    let nick = store::nick(&c.fpr);
    let base = if c.name.is_empty() { "—".to_string() } else { c.name.clone() };
    let name_txt = if nick.is_empty() { base.clone() } else { format!("{base} ({nick})") };
    let mut rows: Vec<(&str, String)> = vec![("Name", name_txt)];
    if !c.email.is_empty() {
        rows.push(("Email", c.email.clone()));
    }
    if !c.keyid.is_empty() {
        rows.push(("Key ID", c.keyid.clone()));
    }
    if let Some(t) = c.created {
        rows.push(("Created", ymd(t)));
    }
    rows.push(("Expires", c.expires.map(ymd).unwrap_or_else(|| "never".into())));
    if !c.fpr.is_empty() {
        rows.push(("Fingerprint", c.fpr.clone()));
    }
    if k.vals.len() != rows.len() {
        k.vals = rows.iter().map(|(_, v)| v.clone()).collect();
    }
    let mut edit = false;
    for (i, (label, v)) in rows.iter().enumerate() {
        k.vals[i] = v.clone();
        y += 2.0;
        if i == 0 {
            // the name hugs its text (monospace), [Edit] right after it
            let ew = (v.chars().count() as f32 + 1.0) * TK_CH + 2.0;
            let r = kv_row(ui, id.with(("kv", i)), fx, y, fw, label, &mut k.vals[i], Some(ew), live);
            let er = ui::word(ui, id.with("edit"), r.max.x + 4.0, y + 3.0, Word::new("Edit"), live);
            edit = live && ui::pressed(ui, &er);
        } else {
            kv_row(ui, id.with(("kv", i)), fx, y, fw, label, &mut k.vals[i], None, live);
        }
        y += 25.0 + 2.0;
    }
    if let Some(st) = c.status() {
        y += 4.0;
        let colr = if st == pgpm_core::keys::Status::Expired { col::RD } else { col::AM };
        ui::text(ui, fx + 3.0, y + 3.0, &format!("(!) {}", st.text()), colr);
        y += 25.0;
    }
    // the actions for this key (copy and delete sit on the box and the header); laid out
    // ahead of the box, so a short window shrinks the box, never the links
    let mut acts: Vec<(&str, u8)> = Vec::new();
    if c.secret && Some(&c.fpr) != app.my_fpr.as_ref() {
        acts.push(("Set as active key", 0));
    }
    acts.push(("Export public", 1));
    if c.secret && !c.on_card {
        acts.push(("Export secret", 2));
    }
    let labels: Vec<&str> = acts.iter().map(|(l, _)| *l).collect();
    let (_, acts_h) = ui::link_layout(ui.ctx(), fw, &labels);
    let acts_y = m.max.y - 12.0 - acts_h;
    y += 12.0;
    let br = Rect::from_min_max(pos2(fx, y), pos2(fx + fw, acts_y - 10.0));
    ui::text_box(ui, id.with("pub"), br, &mut k.pubkey, col::T2, live);
    let cw = ui::word_w(ui.ctx(), "Copy");
    let cr = ui::word(ui, id.with("copy"), br.max.x - 10.0 - cw, br.min.y + 10.0, Word::new("Copy").on(col::INP), live);
    if live && ui::pressed(ui, &cr) {
        copy(ui, k.pubkey.value());
    }
    let words: Vec<Word<'_>> = labels.iter().map(|l| Word::new(l)).collect();
    let (hit, _) = ui::link_row(ui, id.with("acts"), fx, acts_y, fw, &words, live);
    let act = hit.map(|i| acts[i].1);
    if back {
        match k.back {
            Some(i) if i < app.contacts.len() => app.open_chat(i),
            _ => app.welcome(),
        }
        return;
    }
    if del {
        delete_from_tab(app, c.clone());
        return;
    }
    if edit {
        let fpr = c.fpr.clone();
        let msg = format!("A nickname for {base}, shown in your contacts. Leave it empty to remove it.");
        app.ask_string("Nickname", &msg, &nick, move |_app, new| {
            if let Some(n) = new {
                store::set_nick(&fpr, n.trim());
            }
        });
    }
    match act {
        Some(0) => {
            let fpr = c.fpr.clone();
            app.set_active_key(Some(fpr));
            app.refresh();
            app.open_key_view(c);
        }
        Some(1) => crate::profile::export_key(app, &c.fpr, false),
        Some(2) => crate::profile::export_key(app, &c.fpr, true),
        _ => {}
    }
}

fn delete_from_tab(app: &mut App, c: Key) {
    let nm = if !c.name.is_empty() { c.name.clone() } else if !c.keyid.is_empty() { c.keyid.clone() } else { "this key".into() };
    let msg = if c.secret {
        format!("Delete {nm} (including its private key) from the keyring?")
    } else {
        format!("Remove {nm} from the keyring?")
    };
    app.ask("Delete key", &format!("{msg}\nThis cannot be undone."), move |app, yes| {
        if !yes {
            return;
        }
        let r = if c.secret { gpg::delete_secret(&c.fpr) } else { gpg::delete_pub(&c.fpr) };
        if let Err(e) = r {
            app.error("Error", &e);
            return;
        }
        app.active = None;
        app.refresh();
        app.welcome();
    });
}

// -------------------------------------------------------------------------------------------
// Pick one of your keys (moving a key to the card)
// -------------------------------------------------------------------------------------------
pub struct Choose {
    pub title: String,
    pub serial: u64,
}

fn choose(app: &mut App, c: &mut Choose, ui: &mut Ui, m: Rect) {
    let live = app.dialogs.is_empty();
    let id = Id::NULL.with(("choose", c.serial));
    let (back, _) = header(app, ui, m, id.with("hdr"), &c.title, true, None);
    let x = m.min.x + 24.0;
    let mut y = m.min.y + 53.0 + 16.0;
    let mut picked = None;
    let keys: Vec<Key> = app.secret.iter().filter(|k| !k.on_card).cloned().collect();
    for (i, k) in keys.iter().enumerate() {
        y += 3.0;
        let nm = if k.name.is_empty() { k.keyid.clone() } else { k.name.clone() };
        let r = ui::word(ui, id.with(i), x, y, Word::new(&nm), live);
        if live && ui::pressed(ui, &r) {
            picked = Some(k.clone());
        }
        y += LINE + 3.0;
    }
    if back {
        app.open_card();
        return;
    }
    if let Some(k) = picked {
        crate::card::card_action(app, move || gpg::keytocard(&k.fpr, "2"), "Encryption key moved to the card. (Re-detect to confirm.)", None);
    }
}

pub fn choose_secret(app: &mut App, title: &str) {
    let s = app.next_serial();
    app.go(View::Choose(Choose { title: title.into(), serial: s }));
}
