//! The console toolkit, drawn to match PGPM's Tk console look: Cascadia Mono 16px on 19px
//! lines, buttons as bracketed words that invert under the pointer, flat fields that brighten
//! while you type in them, a square scrollbar thumb. Coordinates are Tk's (= egui points).

use std::sync::Arc;

use egui::{Color32, FontFamily, FontId, Galley, Id, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::fonts;

// -------------------------------------------------------------------------------------------
// Palette (cmd's "Campbell" greys) and metrics
// -------------------------------------------------------------------------------------------
pub mod col {
    use egui::Color32;
    pub const BK: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
    pub const INP: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    /// a field while you type in it
    pub const INP_FOCUS: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
    /// a row under the pointer
    pub const HOV: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    /// selection = inverted
    pub const SEL: Color32 = Color32::from_rgb(0xcc, 0xcc, 0xcc);
    pub const ON_FILL: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
    /// the second line of a selected row / key card
    pub const SUB_ON: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
    pub const T1: Color32 = Color32::from_rgb(0xcc, 0xcc, 0xcc);
    pub const T2: Color32 = Color32::from_rgb(0xa8, 0xa8, 0xa8);
    pub const T3: Color32 = Color32::from_rgb(0x76, 0x76, 0x76);
    pub const T4: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);
    pub const WT: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);
    pub const RD: Color32 = Color32::from_rgb(0xe7, 0x48, 0x56);
    pub const AM: Color32 = Color32::from_rgb(0xc1, 0x9c, 0x00);
    /// no green text: a good result is plain grey
    pub const GN: Color32 = T2;
    pub const SCRL: Color32 = Color32::from_rgb(0x76, 0x76, 0x76);
    /// pick lists and tooltips
    pub const POP: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
    /// the floating copy of a dragged contact
    pub const GHOST: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
    /// the update banner
    pub const BANNER: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
}

/// One console line (Cascadia Mono 16px: 15 above the baseline, 4 below).
pub const LINE: f32 = 19.0;
/// Tk's width of "0", the unit of a widget's width in characters.
pub const TK_CH: f32 = 10.0;
/// Two characters: the gap between the words of a button row.
pub const GAP: f32 = 19.0;

pub fn font() -> FontId {
    FontId::new(16.0, FontFamily::Monospace)
}

/// Baseline from the top of a 19px line (ascent and descent rounded to whole pixels).
pub fn baseline() -> f32 {
    let a = (fonts::ASCENT / fonts::UPEM * 16.0).round();
    let d = (fonts::DESCENT / fonts::UPEM * 16.0).round();
    (LINE - (a + d)) / 2.0 + a
}

/// A delay in seconds as a Duration (never negative, infinite or NaN).
pub fn secs(s: f64) -> std::time::Duration {
    std::time::Duration::from_secs_f64(if s.is_finite() { s.clamp(0.0, 3600.0) } else { 3600.0 })
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// -------------------------------------------------------------------------------------------
// Text
// -------------------------------------------------------------------------------------------
pub fn galley(ctx: &egui::Context, t: &str, color: Color32) -> Arc<Galley> {
    ctx.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font(), color))
}

/// Text width in points.
pub fn measure(ctx: &egui::Context, t: &str) -> f32 {
    let fid = font();
    ctx.fonts_mut(|f| t.chars().map(|c| f.glyph_width(&fid, c)).sum())
}

/// A Tk widget's width for `t`: whole pixels, rounded up.
pub fn wi(ctx: &egui::Context, t: &str) -> f32 {
    measure(ctx, t).ceil()
}

/// The baseline of Cascadia's own glyphs in `g` (a fallback glyph may sit on another one).
fn galley_baseline(g: &Galley) -> f32 {
    let Some(r) = g.rows.first() else { return 15.0 };
    let gl = r.row.glyphs.iter().find(|gl| gl.chr.is_ascii_alphanumeric()).or(r.row.glyphs.first());
    gl.map(|gl| r.pos.y + gl.pos.y).unwrap_or(15.0)
}

/// Where to put galley `g` so its text sits in the 19px line starting at `top` where Tk draws
/// it (CoreText's baseline lands a pixel lower than egui's for the same metrics).
pub fn galley_y(g: &Galley, top: f32) -> f32 {
    top + baseline() + 1.0 - galley_baseline(g)
}

/// One line of text whose 19px line box has its top-left at (x, top). Returns its width.
pub fn text(ui: &Ui, x: f32, top: f32, t: &str, color: Color32) -> f32 {
    if t.is_empty() {
        return 0.0;
    }
    let g = galley(ui.ctx(), t, color);
    let w = g.rect.width();
    let pos = pos2(x, galley_y(&g, top));
    ui.painter().galley(pos, g, color);
    w
}

/// Like [`text`] but clipped to `clip`.
pub fn text_clipped(ui: &Ui, x: f32, top: f32, t: &str, color: Color32, clip: Rect) -> f32 {
    if t.is_empty() {
        return 0.0;
    }
    let g = galley(ui.ctx(), t, color);
    let w = g.rect.width();
    let pos = pos2(x, galley_y(&g, top));
    ui.painter().with_clip_rect(clip.intersect(ui.clip_rect())).galley(pos, g, color);
    w
}

/// Text in lines no wider than `max_w` (Tk's -wraplength: breaks at spaces, a word longer
/// than a line is broken anywhere), keeping the text's own newlines.
pub fn wrap(ctx: &egui::Context, t: &str, max_w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in t.split('\n') {
        if max_w <= 0.0 || measure(ctx, para) <= max_w {
            out.push(para.to_string());
            continue;
        }
        let mut line = String::new();
        let mut lw = 0.0;
        for word in para.split_inclusive(' ') {
            let ww = measure(ctx, word.trim_end_matches(' '));
            if lw > 0.0 && lw + ww > max_w {
                out.push(line.trim_end().to_string());
                line.clear();
                lw = 0.0;
            }
            if ww > max_w {
                // a word wider than the line: break it anywhere
                for c in word.chars() {
                    let cw = measure(ctx, &c.to_string());
                    if lw + cw > max_w && !line.is_empty() {
                        out.push(line.clone());
                        line.clear();
                        lw = 0.0;
                    }
                    line.push(c);
                    lw += cw;
                }
                continue;
            }
            line.push_str(word);
            lw += measure(ctx, word);
        }
        out.push(line.trim_end().to_string());
    }
    out
}

/// A Tk label's text block: `lines` drawn from (x, top), 19px apart. Returns the height.
pub fn lines(ui: &Ui, x: f32, top: f32, lines: &[String], color: Color32) -> f32 {
    for (i, l) in lines.iter().enumerate() {
        text(ui, x, top + i as f32 * LINE, l, color);
    }
    lines.len() as f32 * LINE
}

// -------------------------------------------------------------------------------------------
// Words (the console's buttons)
// -------------------------------------------------------------------------------------------
#[derive(Clone, Copy)]
pub struct Word<'a> {
    pub text: &'a str,
    /// resting text colour
    pub fg: Color32,
    /// red under the pointer (delete)
    pub danger: bool,
    pub enabled: bool,
    /// shown selected (inverted), e.g. the open tab
    pub lit: bool,
    /// background at rest (what the word sits on)
    pub bg: Color32,
}

impl<'a> Word<'a> {
    pub fn new(text: &'a str) -> Word<'a> {
        Word { text, fg: col::T1, danger: false, enabled: true, lit: false, bg: col::BK }
    }
    pub fn fg(mut self, c: Color32) -> Self {
        self.fg = c;
        self
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self.fg = col::RD;
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    pub fn lit(mut self, on: bool) -> Self {
        self.lit = on;
        self
    }
    pub fn on(mut self, bg: Color32) -> Self {
        self.bg = bg;
        self
    }
}

pub fn word_w(ctx: &egui::Context, t: &str) -> f32 {
    wi(ctx, &format!("[{t}]"))
}

/// What a press on a widget did this frame (Tk's buttons act on the press, not the release).
pub fn pressed(ui: &Ui, r: &Response) -> bool {
    r.hovered() && ui.input(|i| i.pointer.primary_pressed())
}

/// `[text]` at (x, y): inverted under the pointer (red for a delete), dim when disabled.
/// Returns the response; act on [`pressed`].
pub fn word(ui: &mut Ui, id: Id, x: f32, y: f32, w: Word<'_>, live: bool) -> Response {
    let label = format!("[{}]", w.text);
    let bw = wi(ui.ctx(), &label);
    let r = Rect::from_min_size(pos2(x, y), vec2(bw, LINE));
    let sense = if live && w.enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(r, id, sense);
    let hot = live && w.enabled && resp.hovered();
    let (bg, fg) = if w.lit {
        (Some(col::SEL), col::ON_FILL)
    } else if hot {
        (Some(if w.danger { col::RD } else { col::SEL }), col::ON_FILL)
    } else if !w.enabled {
        (None, col::T4)
    } else {
        (None, w.fg)
    };
    if let Some(bg) = bg {
        ui.painter().rect_filled(r, 0.0, bg);
    } else if w.bg != col::BK {
        ui.painter().rect_filled(r, 0.0, w.bg);
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    text(ui, x, y, &label, fg);
    resp
}

/// A row of words two characters apart from `x`; returns (index pressed, end x).
pub fn word_row(ui: &mut Ui, id: Id, x: f32, y: f32, words: &[Word<'_>], live: bool) -> (Option<usize>, f32) {
    let mut xx = x;
    let mut hit = None;
    for (i, w) in words.iter().enumerate() {
        let r = word(ui, id.with(i), xx, y, *w, live);
        if live && pressed(ui, &r) && w.enabled {
            hit = Some(i);
        }
        xx += r.rect.width() + GAP;
    }
    (hit, xx)
}

// -------------------------------------------------------------------------------------------
// Tooltips (shown 500ms after the pointer settles on a widget)
// -------------------------------------------------------------------------------------------
/// Registers `tip` for `resp`: once the pointer has rested on it for half a second, the tip is
/// drawn centred above it (below when there is no room) at the end of the frame.
pub fn tip(ui: &Ui, resp: &Response, tip: &str) {
    let ctx = ui.ctx();
    let key = resp.id.with("tip-since");
    let now = ctx.input(|i| i.time);
    if resp.hovered() && !ctx.input(|i| i.pointer.any_down()) {
        let since = ctx.data_mut(|d| *d.get_temp_mut_or(key, now));
        if !since.is_finite() {
            // pressed: no tip until the pointer comes back
        } else if now - since >= 0.5 {
            ctx.data_mut(|d| d.insert_temp(Id::NULL.with("tip-show"), (resp.rect, tip.to_string())));
        } else {
            ctx.request_repaint_after(crate::ui::secs(0.5 - (now - since) + 0.01));
        }
    } else {
        ctx.data_mut(|d| d.remove::<f64>(key));
    }
    if resp.hovered() && ctx.input(|i| i.pointer.any_pressed()) {
        // a press hides it until the pointer comes back
        ctx.data_mut(|d| d.insert_temp(key, f64::INFINITY));
    }
}

/// Draws the tooltip registered this frame (call last).
pub fn draw_tip(ctx: &egui::Context, win: Rect) {
    let key = Id::NULL.with("tip-show");
    let Some((r, t)) = ctx.data_mut(|d| {
        let v = d.get_temp::<(Rect, String)>(key);
        d.remove::<(Rect, String)>(key);
        v
    }) else {
        return;
    };
    let tw = wi(ctx, &t);
    let (w, h) = (tw + 22.0, LINE + 12.0);
    let x = (r.center().x - (w / 2.0).floor()).floor().max(2.0);
    let mut y = r.min.y - h - 5.0;
    if y < 4.0 {
        y = r.max.y + 5.0;
    }
    let x = x.min(win.max.x - w).max(win.min.x);
    let rect = Rect::from_min_size(pos2(x, y), vec2(w, h));
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::NULL.with("tip")));
    painter.rect_filled(rect, 0.0, col::BK);
    painter.rect_filled(rect.shrink(1.0), 0.0, col::POP);
    let g = galley(ctx, &t, col::T1);
    let gy = galley_y(&g, y + 6.0);
    painter.galley(pos2(x + 11.0, gy), g, col::T1);
}

// -------------------------------------------------------------------------------------------
// Text fields
// -------------------------------------------------------------------------------------------
/// A multi-line message box (Tk Text in a flat canvas, 10px inside, Text's own 1px pad).
/// The placeholder is not part of the text: it shows (dim) until the box is first focused,
/// and comes back on leaving an empty box when `ph_restore` is set.
#[derive(Clone, Default)]
pub struct TextBox {
    pub text: String,
    pub ph: String,
    pub ph_on: bool,
    pub ph_restore: bool,
    /// a file is being dragged over it
    pub drop_hi: bool,
}

impl TextBox {
    pub fn new(ph: &str, restore: bool) -> TextBox {
        TextBox { text: String::new(), ph: ph.into(), ph_on: !ph.is_empty(), ph_restore: restore, drop_hi: false }
    }
    /// The text (empty while the placeholder shows).
    pub fn value(&self) -> &str {
        &self.text
    }
    pub fn set(&mut self, t: impl Into<String>) {
        self.text = t.into();
        self.ph_on = false;
    }
}

fn layouter(fg: Color32, wrap: bool) -> impl FnMut(&Ui, &dyn egui::TextBuffer, f32) -> Arc<Galley> {
    move |ui: &Ui, buf: &dyn egui::TextBuffer, wrap_w: f32| {
        let mut job = egui::text::LayoutJob::simple(buf.as_str().to_owned(), font(), fg, wrap_w);
        for s in &mut job.sections {
            s.format.line_height = Some(LINE);
        }
        job.wrap.max_width = if wrap { wrap_w } else { f32::INFINITY };
        ui.fonts_mut(|f| f.layout_job(job))
    }
}

/// The box at `rect`. `fg` is the text colour. Returns the TextEdit's response.
pub fn text_box(ui: &mut Ui, id: Id, rect: Rect, tb: &mut TextBox, fg: Color32, live: bool) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    ui.painter().rect_filled(rect, 0.0, if focused { col::INP_FOCUS } else { col::INP });
    if tb.drop_hi {
        ui.painter().rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, col::WT), egui::StrokeKind::Middle);
    }
    let inner = Rect::from_min_max(rect.min + vec2(11.0, 11.0), rect.max - vec2(11.0, 11.0));
    if inner.width() <= 1.0 || inner.height() <= 1.0 {
        return ui.interact(rect, id.with("bg"), Sense::hover());
    }
    let mut lay = layouter(fg, true);
    let te = egui::TextEdit::multiline(&mut tb.text)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(font())
        .text_color(fg)
        .desired_width(inner.width())
        .min_size(vec2(inner.width(), inner.height()))
        .interactive(live)
        .layouter(&mut lay);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt(id.with("area")));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let out = egui::ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut child, |ui| te.show(ui));
    let resp = out.inner.response.response.clone();
    // a click in the 10px margin still focuses the text, as on the Tk canvas
    let bg = ui.interact(rect, id.with("bg"), if live { Sense::click() } else { Sense::hover() });
    if live && bg.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }
    if resp.hovered() || bg.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let now_focused = ui.memory(|m| m.has_focus(id));
    if now_focused && tb.ph_on {
        tb.ph_on = false;
    }
    if !now_focused && focused && tb.ph_restore && tb.text.trim().is_empty() {
        tb.text.clear();
        tb.ph_on = !tb.ph.is_empty();
    }
    if tb.ph_on && tb.text.is_empty() {
        // where the Text widget's own first line sits
        text_clipped(ui, inner.min.x, inner.min.y - 1.0, &tb.ph.clone(), col::T3, inner);
    }
    resp
}

/// A one-line entry: text from (x, y) in a field `w` x `h` (no fill of its own).
pub struct EntryOpts {
    pub fg: Color32,
    pub password: bool,
    pub readonly: bool,
}

impl Default for EntryOpts {
    fn default() -> Self {
        EntryOpts { fg: col::T1, password: false, readonly: false }
    }
}

/// A Tk Entry at `rect` (text vertically centred like Tk: (h - 19) / 2 down).
pub fn entry(ui: &mut Ui, id: Id, rect: Rect, value: &mut String, o: EntryOpts, live: bool) -> egui::text_edit::TextEditOutput {
    if o.readonly {
        // read-only: selectable and copyable, never edited (edits are dropped)
        let before = value.clone();
        let out = entry_inner(ui, id, rect, value, &o, live);
        *value = before;
        return out;
    }
    entry_inner(ui, id, rect, value, &o, live)
}

fn entry_inner(ui: &mut Ui, id: Id, rect: Rect, value: &mut String, o: &EntryOpts, live: bool) -> egui::text_edit::TextEditOutput {
    let off = ((rect.height() - LINE) / 2.0).floor().max(0.0);
    let inner = Rect::from_min_size(rect.min + vec2(1.0, off), vec2(rect.width() - 1.0, LINE));
    let mut lay = layouter(o.fg, false);
    let mut te = egui::TextEdit::singleline(value)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(font())
        .text_color(o.fg)
        .desired_width(inner.width())
        .interactive(live)
        .clip_text(true)
        .layouter(&mut lay);
    if o.password {
        te = te.password(true);
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt(id.with("area")));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    te.show(&mut child)
}

// -------------------------------------------------------------------------------------------
// Scrolling (a square 6px thumb that shows while scrolling, gone after 4s)
// -------------------------------------------------------------------------------------------
/// A scrolled region at `rect` with a thumb on the `left` or right. `draw` gets the Ui and the
/// content's top-left (scrolled), returns the content height.
pub fn scroll_area(ui: &mut Ui, id: Id, rect: Rect, left: bool, draw: impl FnOnce(&mut Ui, Pos2) -> f32) -> f32 {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(id));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let mut area = egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    let geom_key = id.with("geom");
    if !left {
        let want = ui.ctx().data_mut(|d| d.remove_temp::<bool>(Id::NULL.with("snap-scroll-bottom"))).unwrap_or(false);
        if let (true, Some(h)) = (want, ui.ctx().data(|d| d.get_temp::<f32>(geom_key))) {
            area = area.vertical_scroll_offset((h - rect.height()).max(0.0)); // to the end
        }
    }
    let out = area
        .show_viewport(&mut child, |ui, _vp| {
            let origin = ui.max_rect().min;
            let h = draw(ui, origin);
            ui.allocate_rect(Rect::from_min_size(origin, vec2(rect.width(), h.max(1.0))), Sense::hover());
            h
        });
    let content_h = out.inner;
    ui.ctx().data_mut(|d| d.insert_temp(geom_key, content_h));
    let off = out.state.clamped_offset().y;
    // the thumb: shown while the view moves or the content changes, then hidden after 4s
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let key = id.with("thumb");
    let (last_off, last_h, shown_at) = ctx.data(|d| d.get_temp::<(f32, f32, f64)>(key)).unwrap_or((-1.0, -1.0, f64::NEG_INFINITY));
    let changed = (off - last_off).abs() > 0.01 || (content_h - last_h).abs() > 0.01;
    let shown_at = if changed { now } else { shown_at };
    ctx.data_mut(|d| d.insert_temp(key, (off, content_h, shown_at)));
    let view_h = rect.height();
    if content_h > view_h + 0.5 && now - shown_at < 4.0 {
        // Tk's arithmetic: the view as fractions of the content (first, last), in doubles
        let (track, ch) = (view_h as f64, content_h as f64);
        let first = off as f64 / ch;
        let last = (off as f64 + track) / ch;
        let view = last - first;
        let h = (view * track).min(100.0).max(22.0).floor();
        let pad = 9.0;
        let usable = (track - 2.0 * pad - h).max(0.0);
        let range = 1.0 - view;
        let progress = if range > 1e-9 { (first / range).clamp(0.0, 1.0) } else { 0.0 };
        let (h, top) = (h as f32, (pad + progress * usable).floor() as f32);
        let x = if left { rect.min.x + 3.0 } else { rect.max.x - 9.0 };
        ui.painter().rect_filled(Rect::from_min_size(pos2(x, rect.min.y + top), vec2(6.0, h)), 0.0, col::SCRL);
        ctx.request_repaint_after(crate::ui::secs((4.0 - (now - shown_at)).max(0.05)));
    }
    content_h
}

/// Global egui style for the console look.
pub fn install_style(ctx: &egui::Context) {
    // like Tk: a click on a word, a row or the background never takes focus from a field;
    // only clicking another field (or a dialog) moves it
    ctx.options_mut(|o| o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::Vec2::ZERO;
        style.spacing.button_padding = egui::Vec2::ZERO;
        style.spacing.interact_size = egui::Vec2::ZERO;
        style.interaction.selectable_labels = false;
        style.interaction.tooltip_delay = f32::INFINITY;
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.panel_fill = col::BK;
        v.window_fill = col::BK;
        v.extreme_bg_color = col::BK;
        v.override_text_color = None;
        // a console marks a text selection inverted
        v.selection.bg_fill = col::SEL;
        v.selection.stroke = Stroke::new(1.0, col::ON_FILL);
        v.text_cursor.stroke = Stroke::new(2.0, col::T1);
        v.text_cursor.blink = true;
        v.text_cursor.on_duration = 0.6;
        v.text_cursor.off_duration = 0.3;
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = egui::CornerRadius::ZERO;
            w.bg_stroke = Stroke::NONE;
            w.expansion = 0.0;
        }
        let sc = &mut style.spacing.scroll;
        sc.floating = true;
        sc.bar_width = 0.0;
        sc.floating_width = 0.0;
        sc.floating_allocated_width = 0.0;
    });
}

/// Lays out links in a row that wraps like words where the next one doesn't fit `w`; between
/// two links a 25px separator (Tk's "  " label), which never starts or ends a line. Returns
/// each link's (x, y) and the height: 25 for a line holding a separator, else 19.
pub fn link_layout(ctx: &egui::Context, w: f32, labels: &[&str]) -> (Vec<(f32, f32)>, f32) {
    let widths: Vec<f32> = labels.iter().map(|l| word_w(ctx, l)).collect();
    let sep = wi(ctx, "  ") + 6.0;
    let (mut x, mut y, mut lh) = (0.0f32, 0.0f32, 0.0f32);
    let mut pos = Vec::new();
    for (i, lw) in widths.iter().enumerate() {
        if i > 0 && !(x == 0.0 || x + sep + lw > w) {
            x += sep;
            lh = lh.max(25.0);
        }
        if x > 0.0 && x + lw > w {
            x = 0.0;
            y += lh;
            lh = 0.0;
        }
        pos.push((x, y));
        x += lw;
        lh = lh.max(LINE);
    }
    (pos, y + lh)
}

/// Draws [`link_layout`]'s row at (x0, y0); returns (index pressed, height).
pub fn link_row(ui: &mut Ui, id: Id, x0: f32, y0: f32, w: f32, links: &[Word<'_>], live: bool) -> (Option<usize>, f32) {
    let labels: Vec<&str> = links.iter().map(|l| l.text).collect();
    let (pos, h) = link_layout(ui.ctx(), w, &labels);
    let mut hit = None;
    for (i, (l, (x, y))) in links.iter().zip(pos).enumerate() {
        let r = word(ui, id.with(i), x0 + x, y0 + y, *l, live);
        if live && pressed(ui, &r) && l.enabled {
            hit = Some(i);
        }
    }
    (hit, h)
}
