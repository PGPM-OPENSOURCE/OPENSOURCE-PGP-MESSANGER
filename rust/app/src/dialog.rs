//! The console look's dialog: it takes over the window like console output, from the
//! top-left: the title (bright, or the warning / error colour), the message, a field, and the
//! buttons as words. Enter picks the last button, Escape the first.

use egui::{Color32, Id, Rect, Ui, pos2, vec2};

use crate::app::App;
use crate::ui::{self, LINE, Word, col};

type Then = Box<dyn FnOnce(&mut App, i32, Option<String>)>;

pub struct Dialog {
    pub title: String,
    pub message: String,
    pub tone: Color32,
    pub buttons: Vec<(String, i32)>,
    pub entry: Option<String>,
    then: Option<Then>,
    prev_focus: Option<Id>,
    started: bool,
    id: Id,
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Dialog {
    pub fn new(title: &str, message: &str, tone: Color32) -> Dialog {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Dialog {
            title: title.into(),
            message: message.into(),
            tone,
            buttons: vec![("OK".into(), 1)],
            entry: None,
            then: None,
            prev_focus: None,
            started: false,
            id: Id::NULL.with(("dialog", n)),
        }
    }
    pub fn buttons(mut self, b: &[(&str, i32)]) -> Self {
        self.buttons = b.iter().map(|(l, v)| (l.to_string(), *v)).collect();
        self
    }
    pub fn entry(mut self, initial: &str) -> Self {
        self.entry = Some(initial.into());
        self
    }
    pub fn then(mut self, f: impl FnOnce(&mut App, i32, Option<String>) + 'static) -> Self {
        self.then = Some(Box::new(f));
        self
    }
    pub fn with_prev_focus(mut self, f: Option<Id>) -> Self {
        self.prev_focus = f;
        self
    }
}

/// Draws the top dialog over the whole window; a button (or Enter / Escape) closes it.
pub fn draw(app: &mut App, ui: &mut Ui, win: Rect) {
    let Some(d) = app.dialogs.last_mut() else { return };
    ui.painter().rect_filled(win, 0.0, col::BK);
    let x = win.min.x + 16.0;
    let w = win.width() - 32.0;
    let mut y = win.min.y + 14.0;
    let title_c = if d.tone == col::T1 { col::WT } else { d.tone };
    ui::text(ui, x + 3.0, y + 3.0, &d.title, title_c);
    y += 25.0;
    if !d.message.is_empty() {
        y += 19.0;
        let wl = (win.width() - 40.0).max(300.0);
        let ls = ui::wrap(ui.ctx(), &d.message, wl);
        let h = ui::lines(ui, x + 3.0, y + 3.0, &ls, col::T1);
        y += h + 6.0;
    }
    let mut picked: Option<i32> = None;
    let entry_id = d.id.with("entry");
    if let Some(text) = d.entry.as_mut() {
        y += 19.0;
        let r = Rect::from_min_size(pos2(x, y), vec2(w, 29.0));
        ui.painter().rect_filled(r, 0.0, col::INP_FOCUS);
        let er = Rect::from_min_size(pos2(x + 10.0, y + 4.0), vec2(w - 20.0, 21.0));
        let out = ui::entry(ui, entry_id, er, text, ui::EntryOpts::default(), true);
        if !d.started {
            // focused, with any initial text selected (typing replaces it)
            ui.memory_mut(|m| m.request_focus(entry_id));
            let mut st = out.state.clone();
            let n = text.chars().count();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(n),
            )));
            st.store(ui.ctx(), entry_id);
        }
        y += 29.0;
    }
    y += 19.0;
    let words: Vec<Word<'_>> = d.buttons.iter().map(|(l, _)| Word::new(l)).collect();
    let (hit, _) = ui::word_row(ui, d.id.with("btns"), x, y, &words, true);
    if let Some(i) = hit {
        picked = Some(d.buttons[i].1);
    }
    let _ = LINE;
    if d.started && picked.is_none() {
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if enter {
            picked = d.buttons.last().map(|b| b.1);
        } else if esc {
            picked = d.buttons.first().map(|b| b.1);
        }
    }
    d.started = true;
    if let Some(v) = picked {
        let d = app.dialogs.pop().unwrap();
        let text = d.entry.clone();
        if let Some(f) = d.prev_focus {
            if app.dialogs.is_empty() {
                ui.memory_mut(|m| m.request_focus(f));
            }
        } else {
            ui.memory_mut(|m| m.request_focus(Id::NULL));
        }
        if let Some(then) = d.then {
            then(app, v, text);
        }
        ui.ctx().request_repaint();
    }
}
