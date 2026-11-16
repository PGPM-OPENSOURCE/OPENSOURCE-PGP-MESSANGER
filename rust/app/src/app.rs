//! App state: the keyring as GnuPG lists it, the panel on screen, the dialogs over it, and
//! the work that runs off the UI thread (gpg calls that may wait on a PIN, the network).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use egui::{Id, Rect, Ui, pos2, vec2};
use pgpm_core::gpg::{self, CardInfo};
use pgpm_core::keys::Key;
use pgpm_core::store;

use crate::card::CardPanel;
use crate::dialog::Dialog;
use crate::panels::{Add, Chat, Choose, Decrypt, Encrypt, KeyView};
use crate::profile::Profile;
use crate::ui::{self, col};

/// Work finished off the UI thread, applied on it.
pub type Done = Box<dyn FnOnce(&mut App) + Send>;

pub enum View {
    Welcome,
    Chat(Chat),
    Encrypt(Encrypt),
    Decrypt(Decrypt),
    Add(Add),
    Key(KeyView),
    Choose(Choose),
    Card(CardPanel),
    Info(Profile),
}

impl View {
    /// The panel's id ("re-clicking the open tab does nothing"); the key viewer and the
    /// pick-a-key list have none.
    pub fn id(&self) -> Option<String> {
        match self {
            View::Welcome => Some("welcome".into()),
            View::Chat(c) => Some(format!("chat:{}", c.fpr)),
            View::Encrypt(_) => Some("encrypt".into()),
            View::Decrypt(_) => Some("decrypt".into()),
            View::Add(_) => Some("add".into()),
            View::Card(_) => Some("card".into()),
            View::Info(_) => Some("info".into()),
            View::Key(_) | View::Choose(_) => None,
        }
    }
}

/// A file being dragged over the window.
#[derive(Default)]
pub struct DropState {
    /// the message field under the pointer (its rect), if any
    pub over: Option<Rect>,
}

pub struct App {
    pub ctx: egui::Context,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    pub gpg_ok: bool,
    /// your own keys (secret) and everyone else's (public)
    pub secret: Vec<Key>,
    pub contacts: Vec<Key>,
    /// the active key
    pub my_fpr: Option<String>,
    /// last known smartcard presence (None = not yet probed)
    pub card_present: Option<bool>,
    /// "Use this card" was clicked (reset on unplug)
    pub card_in_use: bool,
    pub card_serial: Option<String>,
    /// the card details last shown (instant on the next visit while in use)
    pub card_info: Option<CardInfo>,
    /// the highlighted contact (index into `contacts`)
    pub active: Option<usize>,
    pub search: String,
    pub view: View,
    pub cur_id: Option<String>,
    /// a new panel to show after this frame
    pub nav: Option<View>,
    pub serial: u64,
    pub dialogs: Vec<Dialog>,
    pub update_tag: Option<String>,
    pub drag: Option<crate::sidebar::Drag>,
    pub drop: DropState,
    watch_busy: bool,
    next_watch: f64,
    update_at: Option<f64>,
    menu_installed: bool,
    pub snap: Option<crate::snap::Snap>,
    pub script: Option<crate::script::Script>,
    /// the pointer a test script holds (file drops)
    pub test_pointer: Option<egui::Pos2>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, snap: Option<crate::snap::Snap>, script: Option<crate::script::Script>) -> App {
        crate::fonts::install(&cc.egui_ctx, false);
        ui::install_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let gpg_ok = gpg::available();
        let mut app = App {
            ctx: cc.egui_ctx.clone(),
            tx,
            rx,
            gpg_ok,
            secret: Vec::new(),
            contacts: Vec::new(),
            my_fpr: store::my_fpr(),
            card_present: None,
            card_in_use: false,
            card_serial: None,
            card_info: None,
            active: None,
            search: String::new(),
            view: View::Welcome,
            cur_id: Some("welcome".into()),
            nav: None,
            serial: 0,
            dialogs: Vec::new(),
            update_tag: None,
            drag: None,
            drop: DropState::default(),
            watch_busy: false,
            next_watch: 0.0,
            update_at: Some(1.5),
            menu_installed: false,
            snap: None,
            script,
            test_pointer: None,
        };
        store::migrate_legacy();
        app.reload_keys();
        store::log(&format!("ui_font='Cascadia Mono' engine=rust gpg={:?}", gpg::path()));
        if let Some(s) = snap {
            crate::snap::setup(&mut app, &s);
            app.update_at = None;
            app.snap = Some(s);
        }
        app
    }

    // ---------------------------------------------------------------------------------------
    // Keyring (GnuPG is the source of truth, like Kleopatra)
    // ---------------------------------------------------------------------------------------
    pub fn reload_keys(&mut self) {
        self.gpg_ok = gpg::available();
        let mut secret = if self.gpg_ok { gpg::list_secret() } else { Vec::new() };
        let mut contacts = if self.gpg_ok { gpg::list_public() } else { Vec::new() };
        // A keycard leaves a "stub" secret key behind when unplugged: hide the on-card keys
        // (and their public entries) unless the card is in AND "Use this card" was clicked.
        if !(self.card_present == Some(true) && self.card_in_use) {
            let gone: Vec<String> = secret.iter().filter(|k| k.on_card).map(|k| k.fpr.clone()).collect();
            if !gone.is_empty() {
                secret.retain(|k| !k.on_card);
                contacts.retain(|c| !gone.contains(&c.fpr));
            }
        }
        // your own keys are your identities, not contacts
        let own: Vec<String> = secret.iter().map(|k| k.fpr.clone()).collect();
        contacts.retain(|c| !own.contains(&c.fpr));
        // the manual drag order; keys not in it keep gpg's order, at the bottom
        let order = store::contact_order();
        if !order.is_empty() {
            let big = order.len();
            contacts.sort_by_key(|c| order.iter().position(|f| *f == c.fpr).unwrap_or(big));
        }
        self.secret = secret;
        self.contacts = contacts;
        store::log(&format!(
            "reload_keys: gpg={:?} avail={} secret={} contacts={} card_present={:?}",
            gpg::path(),
            self.gpg_ok,
            self.secret.len(),
            self.contacts.len(),
            self.card_present
        ));
        if !self.secret.iter().any(|k| Some(&k.fpr) == self.my_fpr.as_ref()) {
            // a keycard key never becomes active on its own: only when picked
            self.my_fpr = self.secret.iter().find(|k| !k.on_card).map(|k| k.fpr.clone());
            store::set_my_fpr(self.my_fpr.as_deref());
        }
    }

    /// Reload keys from gpg (the list is drawn from them).
    pub fn refresh(&mut self) {
        self.reload_keys();
        store::log(&format!("rebuild_list: contacts={} rows={} filter={:?}", self.contacts.len(), self.contacts.len(), self.search_filter()));
    }

    pub fn search_filter(&self) -> String {
        self.search.trim().to_lowercase()
    }

    pub fn set_active_key(&mut self, fpr: Option<String>) {
        self.my_fpr = fpr;
        store::set_my_fpr(self.my_fpr.as_deref());
    }

    pub fn active_key(&self) -> Option<&Key> {
        self.secret.iter().find(|k| Some(&k.fpr) == self.my_fpr.as_ref())
    }

    pub fn has_key(&self) -> bool {
        self.my_fpr.is_some()
    }

    /// A keycard is plugged in but "Use this card" wasn't clicked yet.
    pub fn card_waiting(&self) -> bool {
        self.card_present == Some(true) && !self.card_in_use
    }

    /// What the contact list shows: the nickname if set, else the key's own name.
    pub fn shown_name(&self, c: &Key) -> String {
        let nick = store::nick(&c.fpr);
        if !nick.is_empty() {
            return nick;
        }
        if !c.name.is_empty() {
            return c.name.clone();
        }
        if !c.email.is_empty() {
            return c.email.clone();
        }
        c.keyid.chars().take(16).collect()
    }

    pub fn contact_index(&self, fpr: &str) -> Option<usize> {
        self.contacts.iter().position(|c| c.fpr == fpr)
    }

    /// label -> fingerprint of the keys a message can be encrypted to (contacts first, so the
    /// default pick is a person; "Only me" last). Expired / revoked / sign-only keys are left
    /// out: gpg refuses them.
    pub fn recipients(&self) -> Vec<(String, String)> {
        let mut opts: Vec<(String, String)> = Vec::new();
        for c in &self.contacts {
            if !c.can_encrypt {
                continue;
            }
            let mut label = self.shown_name(c);
            if c.secret {
                label += "  (me)";
            }
            if !label.is_empty() && !opts.iter().any(|(l, _)| *l == label) {
                opts.push((label, c.fpr.clone()));
            }
        }
        if let Some(me) = &self.my_fpr {
            if !opts.iter().any(|(l, _)| l == "Only me") {
                opts.push(("Only me".into(), me.clone()));
            }
        }
        opts
    }

    /// Why gpg can't encrypt to one of `fprs` (it only says "Unusable public key"), in plain
    /// words, or None when all of them are fine.
    pub fn cant_encrypt(&self, fprs: &[String]) -> Option<String> {
        for fpr in fprs {
            let Some(k) = self.contacts.iter().chain(self.secret.iter()).find(|k| &k.fpr == fpr) else { continue };
            if k.can_encrypt {
                continue;
            }
            let why = if k.validity == "r" {
                "was revoked".to_string()
            } else if k.expired() {
                format!("expired on {}", pgpm_core::keys::ymd(k.expires.unwrap_or(0)))
            } else {
                "has no usable encryption subkey".to_string()
            };
            let nm = if !k.name.is_empty() { k.name.clone() } else if !k.email.is_empty() { k.email.clone() } else { k.keyid.clone() };
            if k.secret {
                return Some(format!("Your key {nm} {why}.\n\nPick or create another key in Profile & Keys."));
            }
            return Some(format!("{nm}'s key {why}, so messages can't be encrypted to it.\n\nImport a current public key for them."));
        }
        None
    }

    /// Explains why there's no active key and what to click, instead of acting.
    pub fn no_key_warning(&mut self, action: &str) {
        let msg = if self.card_waiting() {
            format!("Your keycard isn't in use.\n\nOpen the keycard panel and click USE THIS CARD to {action}.")
        } else {
            format!("You have no active key.\n\nPick or create one in Profile & Keys to {action}.")
        };
        self.warn("No active key", &msg);
    }

    pub fn open_key_setup(&mut self) {
        if self.card_waiting() { self.open_card() } else { self.open_info() }
    }

    // ---------------------------------------------------------------------------------------
    // Panels
    // ---------------------------------------------------------------------------------------
    pub fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    /// Shows `view` after this frame (builds it fresh, like the Tk panels).
    pub fn go(&mut self, view: View) {
        self.cur_id = view.id();
        self.nav = Some(view);
    }

    /// Navigates unless `id` is already on screen (re-clicking the open tab keeps typed text).
    pub fn show(&mut self, id: &str, make: impl FnOnce(&mut App) -> View) {
        if self.cur_id.as_deref() == Some(id) {
            return;
        }
        let v = make(self);
        self.go(v);
    }

    pub fn welcome(&mut self) {
        self.go(View::Welcome);
    }

    pub fn open_chat(&mut self, idx: usize) {
        self.active = Some(idx);
        let Some(c) = self.contacts.get(idx).cloned() else { return };
        let nm = self.shown_name(&c);
        let nm = if nm.is_empty() { "Key".to_string() } else { nm };
        let s = self.next_serial();
        self.go(View::Chat(Chat::new(&c.fpr, &nm, s)));
    }

    pub fn open_encrypt(&mut self) {
        let s = self.next_serial();
        let v = Encrypt::new(self, s);
        self.go(View::Encrypt(v));
    }

    pub fn open_decrypt(&mut self) {
        let s = self.next_serial();
        self.go(View::Decrypt(Decrypt::new(s)));
    }

    pub fn open_add(&mut self) {
        let s = self.next_serial();
        self.go(View::Add(Add::new(s)));
    }

    pub fn open_card(&mut self) {
        let v = CardPanel::open(self);
        self.go(View::Card(v));
    }

    pub fn open_info(&mut self) {
        let s = self.next_serial();
        self.go(View::Info(Profile::new(s)));
    }

    pub fn open_key_view(&mut self, c: Key) {
        let back = self.active;
        let s = self.next_serial();
        self.go(View::Key(KeyView::new(c, back, s)));
    }

    // ---------------------------------------------------------------------------------------
    // Dialogs (drawn over the whole window, like console output)
    // ---------------------------------------------------------------------------------------
    pub fn dialog(&mut self, d: Dialog) {
        let prev = self.ctx.memory(|m| m.focused());
        self.dialogs.push(d.with_prev_focus(prev));
    }

    pub fn info(&mut self, title: &str, msg: &str) {
        self.dialog(Dialog::new(title, msg, col::T1));
    }

    pub fn warn(&mut self, title: &str, msg: &str) {
        self.dialog(Dialog::new(title, msg, col::AM));
    }

    pub fn error(&mut self, title: &str, msg: &str) {
        self.dialog(Dialog::new(title, msg, col::RD));
    }

    /// [No] [Yes]
    pub fn ask(&mut self, title: &str, msg: &str, then: impl FnOnce(&mut App, bool) + 'static) {
        self.dialog(
            Dialog::new(title, msg, col::T1)
                .buttons(&[("No", 0), ("Yes", 1)])
                .then(move |app, v, _| then(app, v == 1)),
        );
    }

    /// [Cancel] [OK] with a field; `then` gets the text, or None on Cancel.
    pub fn ask_string(&mut self, title: &str, prompt: &str, initial: &str, then: impl FnOnce(&mut App, Option<String>) + 'static) {
        self.dialog(
            Dialog::new(title, prompt, col::T1)
                .buttons(&[("Cancel", 0), ("OK", 1)])
                .entry(initial)
                .then(move |app, v, text| then(app, if v == 1 { text } else { None })),
        );
    }

    // ---------------------------------------------------------------------------------------
    // Work off the UI thread
    // ---------------------------------------------------------------------------------------
    /// Runs `work` on a thread, then `then` with its result back on the UI thread.
    pub fn spawn<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, then: impl FnOnce(&mut App, T) + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let r = work();
            let _ = tx.send(Box::new(move |app: &mut App| then(app, r)));
            ctx.request_repaint();
        });
    }

    fn run_done(&mut self) {
        while let Ok(f) = self.rx.try_recv() {
            f(self);
        }
    }

    /// Live keycard watch: plugging / unplugging refreshes what's shown.
    fn card_watch(&mut self, now: f64) {
        if !self.gpg_ok || self.watch_busy || now < self.next_watch || self.snap.is_some() {
            if !self.watch_busy && self.gpg_ok && self.snap.is_none() {
                self.ctx.request_repaint_after(crate::ui::secs((self.next_watch - now).max(0.05)));
            }
            return;
        }
        self.watch_busy = true;
        self.spawn(|| gpg::card_status().present, |app, present| {
            app.watch_busy = false;
            app.next_watch = app.ctx.input(|i| i.time) + 2.5;
            if Some(present) != app.card_present {
                app.card_present = Some(present);
                if !present {
                    app.card_in_use = false; // unplugged: the next insert needs "Use this card"
                }
                app.reload_keys();
                app.on_keys_changed();
            }
        });
    }

    /// A keycard was plugged / unplugged: refresh the panels that show keys.
    fn on_keys_changed(&mut self) {
        match self.cur_id.as_deref() {
            Some("info") => self.open_info(),
            Some("card") => self.open_card(),
            _ => {}
        }
    }

    fn check_update(&mut self, now: f64) {
        let Some(at) = self.update_at else { return };
        if now < at {
            self.ctx.request_repaint_after(crate::ui::secs(at - now));
            return;
        }
        self.update_at = None;
        self.spawn(pgpm_core::net::newer_release, |app, tag| {
            if let Some(t) = tag {
                crate::fonts::install(&app.ctx, true); // the banner's ⬆
                app.update_tag = Some(t);
            }
        });
    }

    /// Loads a text file into the message field of the open panel.
    pub fn load_file_into_view(&mut self, path: &std::path::Path) {
        let data = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                let msg = io_error(&e, path);
                self.error("Read Error", &msg);
                return;
            }
        };
        match &mut self.view {
            View::Chat(c) => c.text.set(data),
            View::Encrypt(e) => e.text.set(data),
            View::Decrypt(d) => d.text.set(data),
            _ => {}
        }
    }

    /// A file dropped on the window: it lands in the message field it was dropped on.
    fn handle_drop(&mut self, ctx: &egui::Context) {
        let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        let field = match &self.view {
            View::Chat(c) => c.field,
            View::Encrypt(e) => e.field,
            View::Decrypt(d) => d.field,
            _ => None,
        };
        let pointer = self.test_pointer.or_else(|| crate::macos_pointer(ctx));
        let over = field.filter(|r| pointer.is_some_and(|p| r.contains(p)));
        if hovering {
            self.drop.over = over;
            ctx.request_repaint_after(Duration::from_millis(60));
        } else if dropped.is_empty() {
            self.drop.over = None;
        }
        if let Some(f) = dropped.first() {
            let target = over.or(self.drop.over);
            self.drop.over = None;
            if let Some(path) = Some(f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()) {
                store::log(&format!(
                    "on_drop: path={:?} target={}",
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    if target.is_some() { "field" } else { "none" }
                ));
                if target.is_some() && path.is_file() {
                    self.load_file_into_view(&path);
                }
            }
        }
        let hi = self.drop.over.is_some();
        match &mut self.view {
            View::Chat(c) => c.text.drop_hi = hi,
            View::Encrypt(e) => e.text.drop_hi = hi,
            View::Decrypt(d) => d.text.drop_hi = hi,
            _ => {}
        }
    }

    fn apply_nav(&mut self) {
        if let Some(v) = self.nav.take() {
            self.cur_id = v.id();
            self.view = v;
            self.drop.over = None;
            // the old panel's fields are gone (the sidebar's search keeps its focus)
            let search = Id::NULL.with("search");
            if let Some(f) = self.ctx.memory(|m| m.focused()) {
                if f != search {
                    self.ctx.memory_mut(|m| m.surrender_focus(f));
                }
            }
            self.ctx.request_repaint();
        }
    }

    /// The update banner (33px, above everything); returns its height.
    fn banner(&mut self, ui: &mut Ui, win: Rect) -> f32 {
        let Some(tag) = self.update_tag.clone() else { return 0.0 };
        let r = Rect::from_min_size(win.min, vec2(win.width(), 33.0));
        ui.painter().rect_filled(r, 0.0, col::BANNER);
        let t = format!("⬆  PGPM v{tag} is available — click to download");
        let tw = ui::wi(ui.ctx(), &t);
        let lr = Rect::from_min_size(win.min + vec2(10.0, 4.0), vec2(tw + 6.0, 25.0));
        let resp = ui.interact(lr, Id::NULL.with("update-link"), egui::Sense::click());
        ui::text(ui, lr.min.x + 3.0, lr.min.y + 3.0, &t, col::WT);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if self.dialogs.is_empty() && ui::pressed(ui, &resp) {
            let url = format!("https://github.com/{}/releases/latest", pgpm_core::GITHUB_REPO);
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        let xw = ui::wi(ui.ctx(), "✕");
        let xr = Rect::from_min_size(pos2(win.max.x - 10.0 - xw - 6.0, win.min.y + 4.0), vec2(xw + 6.0, 25.0));
        let xresp = ui.interact(xr, Id::NULL.with("update-close"), egui::Sense::click());
        ui::text(ui, xr.min.x + 3.0, xr.min.y + 3.0, "✕", col::WT);
        if xresp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if self.dialogs.is_empty() && ui::pressed(ui, &xresp) {
            self.update_tag = None;
        }
        33.0
    }
}

/// An I/O error as Python worded it ("[Errno 2] No such file or directory: '<path>'").
pub fn io_error(e: &std::io::Error, path: &std::path::Path) -> String {
    match e.raw_os_error() {
        Some(n) => {
            let msg = e.to_string();
            let msg = msg.split(" (os error").next().unwrap_or(&msg).to_string();
            format!("[Errno {n}] {msg}: '{}'", path.display())
        }
        None => e.to_string(),
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        col::BK.to_normalized_gamma_f32()
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        #[cfg(target_os = "macos")]
        raw_input.events.extend(crate::macos::take_menu_events());
        if let Some(p) = self.snap.as_ref().and_then(|s| s.hover) {
            raw_input.events.push(egui::Event::PointerMoved(p));
        }
        let now = raw_input.time.unwrap_or(0.0);
        crate::script::step(self, raw_input, now);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "macos")]
        if !self.menu_installed {
            self.menu_installed = true;
            crate::macos::install(&ctx);
        }
        // Windows: the title bar drawn dark (also in light mode), like the Tk console look's
        #[cfg(windows)]
        if !self.menu_installed {
            self.menu_installed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
        }
        #[cfg(target_os = "macos")]
        if crate::macos::take_about() {
            self.info(&format!("PGPM v{}", pgpm_core::APP_VERSION), &format!("https://github.com/{}", pgpm_core::GITHUB_REPO));
        }
        let now = ctx.input(|i| i.time);
        self.run_done();
        self.card_watch(now);
        self.check_update(now);
        self.handle_drop(&ctx);

        let win = ui.max_rect();
        ui.painter().rect_filled(win, 0.0, col::BK);
        let live = self.dialogs.is_empty();
        if live {
            let top = self.banner(ui, win);
            let body = Rect::from_min_max(pos2(win.min.x, win.min.y + top), win.max);
            let side = Rect::from_min_size(body.min, vec2(220.0, body.height()));
            let main = Rect::from_min_max(pos2(body.min.x + 221.0, body.min.y), body.max);
            crate::sidebar::draw(self, ui, side);
            let mut view = std::mem::replace(&mut self.view, View::Welcome);
            crate::panels::draw(self, &mut view, ui, main);
            if self.nav.is_none() {
                self.view = view;
            }
            self.apply_nav();
            crate::sidebar::draw_ghost(self, ui);
        } else {
            crate::dialog::draw(self, ui, win);
            self.apply_nav();
        }
        ui::draw_tip(&ctx, win);
        crate::snap::tick(self, &ctx);
        if self.script.is_some() {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }
}
