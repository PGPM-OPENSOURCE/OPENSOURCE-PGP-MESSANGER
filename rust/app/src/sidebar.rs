//! The sidebar: the search field, the contact list (click to open, press-and-drag to
//! reorder, [i] for the key) and the menu of tabs at the bottom.

use egui::{Color32, Id, Rect, Sense, Ui, pos2, vec2};
use pgpm_core::store;

use crate::app::App;
use crate::ui::{self, LINE, Word, col};

/// A contact being dragged to a new place.
pub struct Drag {
    pub fpr: String,
    /// grab point inside the row
    pub off: f32,
    /// the row's height (with its 1px separator)
    pub gh: f32,
    /// where the row would drop (among the other visible rows)
    pub gap_idx: usize,
    /// (old slot, new slot, start time): the gap glides between them
    pub glide: Option<(usize, usize, f64)>,
    pub pointer_y: f32,
    pub list: Rect,
    pub name: String,
    pub sub: String,
}

const FADE_PX: f32 = 40.0;
const FADE_MAX: f32 = 0.8;
const GLIDE_SECS: f64 = 0.075;

/// One contact row's content.
struct RowInfo {
    idx: usize,
    fpr: String,
    name: String,
    sub: String,
    badge: Option<(String, Color32)>,
}

impl RowInfo {
    fn lines(&self) -> usize {
        1 + usize::from(!self.sub.is_empty()) + usize::from(self.badge.is_some())
    }
    /// the row (lines + 10px above and below) plus its 1px separator
    fn holder_h(&self) -> f32 {
        self.lines() as f32 * LINE + 20.0 + 1.0
    }
}

fn rows(app: &App) -> Vec<RowInfo> {
    let q = app.search_filter();
    let mut out = Vec::new();
    for (i, c) in app.contacts.iter().enumerate() {
        let nick = store::nick(&c.fpr);
        let hay = [nick.as_str(), &c.name, &c.email, &c.keyid, &c.fpr].join(" ").to_lowercase();
        if !q.is_empty() && !hay.contains(&q) {
            continue;
        }
        let mut name = app.shown_name(c);
        if c.secret {
            name += "  (you)";
        }
        let sub = if !c.email.is_empty() {
            c.email.clone()
        } else if !c.keyid.is_empty() {
            format!("{}…", c.keyid.chars().take(16).collect::<String>())
        } else {
            String::new()
        };
        let badge = c.status().map(|s| {
            let col = if s == pgpm_core::keys::Status::Expired { col::RD } else { col::AM };
            (format!("(!) {}", s.text()), col)
        });
        out.push(RowInfo { idx: i, fpr: c.fpr.clone(), name, sub, badge });
    }
    out
}

pub fn draw(app: &mut App, ui: &mut Ui, side: Rect) {
    let x0 = side.min.x;
    let top = side.min.y;
    let live = app.dialogs.is_empty();
    // ── search field (filters by nickname, name, email, key id, fingerprint)
    let pill = Rect::from_min_size(pos2(x0 + 12.0, top + 11.0), vec2(196.0, 30.0));
    let sid = Id::NULL.with("search");
    let focused = ui.memory(|m| m.has_focus(sid));
    ui.painter().rect_filled(pill, 0.0, if focused { col::INP_FOCUS } else { col::INP });
    let er = Rect::from_min_size(pos2(x0 + 22.0, top + 16.0), vec2(174.0, 20.0));
    let out = ui::entry(ui, sid, er, &mut app.search, ui::EntryOpts::default(), live);
    let pill_resp = ui.interact(pill, sid.with("pill"), Sense::click());
    if live && pill_resp.clicked() {
        ui.memory_mut(|m| m.request_focus(sid));
    }
    if out.response.changed() {
        store::log(&format!("apply_filter: q={:?}", app.search_filter()));
    }
    if !ui.memory(|m| m.has_focus(sid)) && app.search.is_empty() {
        // the entry's own text position
        ui::text(ui, er.min.x + 1.0, er.min.y - 1.0, "Search", col::T3);
    }

    // ── contact list
    let list = Rect::from_min_max(pos2(x0, top + 52.0), pos2(x0 + 220.0, side.max.y - 118.0));
    let infos = rows(app);
    let drag_fpr = app.drag.as_ref().map(|d| d.fpr.clone());
    let now = ui.input(|i| i.time);
    // the rows drawn: the dragged one is out of the flow, a gap (gliding) shows where it drops
    let others: Vec<&RowInfo> = infos.iter().filter(|r| Some(&r.fpr) != drag_fpr.as_ref()).collect();
    let mut gaps: Vec<(usize, f32)> = Vec::new();
    if let Some(d) = &app.drag {
        match d.glide {
            Some((old, new, t0)) => {
                let k = (((now - t0) / GLIDE_SECS) * 5.0).floor() as i32 + 1;
                let grown = (d.gh * (k.min(5) as f32) / 5.0).round().clamp(1.0, d.gh);
                gaps.push((new, grown));
                if grown < d.gh {
                    gaps.push((old, d.gh - grown));
                    ui.ctx().request_repaint();
                }
            }
            None => gaps.push((d.gap_idx, d.gh)),
        }
    }
    let mut pressed_row: Option<(usize, String)> = None;
    let mut info_click: Option<usize> = None;
    let mut drag_start: Option<(String, f32, f32, String, String)> = None;
    let mut drag_moved: Option<f32> = None;
    let mut drag_end = false;
    let active = app.active;
    let dragging = app.drag.is_some();
    let mut mids: Vec<f32> = Vec::new();
    ui::scroll_area(ui, Id::NULL.with("clist"), list, true, |ui, o| {
        let mut y = o.y;
        let gap_before = |k: usize, gaps: &[(usize, f32)]| gaps.iter().filter(|(i, _)| *i == k).map(|(_, h)| *h).sum::<f32>();
        for (k, r) in others.iter().enumerate() {
            y += gap_before(k, &gaps);
            let hh = r.holder_h();
            let row = Rect::from_min_size(pos2(o.x + 12.0, y), vec2(196.0, hh - 1.0));
            mids.push(y + hh / 2.0);
            let id = Id::NULL.with(("row", &r.fpr));
            let resp = ui.interact(row, id, if live { Sense::click_and_drag() } else { Sense::hover() });
            let is_active = active == Some(r.idx);
            let ib_w = ui::word_w(ui.ctx(), "i");
            let ib = Rect::from_min_size(pos2(row.max.x - 10.0 - ib_w, y + ((hh - 1.0 - LINE) / 2.0).floor()), vec2(ib_w, LINE));
            let over_ib = ui.rect_contains_pointer(ib);
            let hover = live && !dragging && !is_active && (resp.hovered() || over_ib);
            let bg = if is_active { col::SEL } else if hover { col::HOV } else { col::BK };
            if bg != col::BK {
                ui.painter().rect_filled(row, 0.0, bg);
            }
            if resp.hovered() && live {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            // text, dimmed toward the background near the list's bottom edge
            let fade_start = list.max.y - FADE_PX;
            let faded = |c: Color32, cy: f32| {
                if is_active {
                    return c;
                }
                let t = ((cy - fade_start) / FADE_PX).clamp(0.0, 1.0) * FADE_MAX;
                ui::mix(c, col::BK, t)
            };
            let clip = Rect::from_min_max(pos2(row.min.x + 10.0, row.min.y + 10.0), pos2(ib.min.x - 10.0, row.max.y - 10.0));
            let mut ly = row.min.y + 10.0;
            let name_c = if is_active { col::ON_FILL } else { faded(col::T1, ly + LINE / 2.0) };
            ui::text_clipped(ui, row.min.x + 11.0, ly, &r.name, name_c, clip);
            ly += LINE;
            if !r.sub.is_empty() {
                let c = if is_active { col::SUB_ON } else { faded(col::T3, ly + LINE / 2.0) };
                ui::text_clipped(ui, row.min.x + 11.0, ly, &r.sub, c, clip);
                ly += LINE;
            }
            if let Some((b, c)) = &r.badge {
                let c = faded(*c, ly + LINE / 2.0);
                ui::text_clipped(ui, row.min.x + 11.0, ly, b, c, clip);
            }
            // [i]: the key's details (and delete)
            let w = Word::new("i").fg(if is_active { col::ON_FILL } else { col::T3 }).on(bg);
            let iresp = ui::word(ui, id.with("i"), ib.min.x, ib.min.y, w, live && !dragging);
            ui::tip(ui, &iresp, "Key info");
            if live && !dragging && ui::pressed(ui, &iresp) {
                info_click = Some(r.idx);
            } else if live {
                if resp.drag_started() && !over_ib {
                    let p = ui.input(|i| i.pointer.press_origin()).unwrap_or(row.min);
                    drag_start = Some((r.fpr.clone(), p.y - row.min.y, hh, r.name.clone(), r.sub.clone()));
                } else if resp.clicked() && !over_ib {
                    pressed_row = Some((r.idx, r.fpr.clone()));
                }
            }
            if resp.dragged() || resp.drag_stopped() {
                if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
                    drag_moved = Some(p.y);
                }
            }
            if resp.drag_stopped() {
                drag_end = true;
            }
            y += hh;
        }
        y += gap_before(others.len(), &gaps);
        y - o.y
    });

    // ── drag-to-reorder
    if let Some((fpr, off, gh, name, sub)) = drag_start {
        let idx = infos.iter().position(|r| r.fpr == fpr).unwrap_or(0);
        app.drag = Some(Drag { fpr, off, gh, gap_idx: idx, glide: None, pointer_y: 0.0, list, name, sub });
    }
    if let Some(d) = app.drag.as_mut() {
        if let Some(py) = drag_moved.or_else(|| ui.input(|i| i.pointer.latest_pos().map(|p| p.y))) {
            d.pointer_y = py;
        }
        d.list = list;
        if let Some((_, _, t0)) = d.glide {
            if now - t0 >= GLIDE_SECS {
                d.glide = None;
            }
        }
        if d.glide.is_none() && !drag_end {
            // the slot whose middle is below the pointer (rows as laid out, with the gap)
            let mut idx = mids.len();
            for (i, m) in mids.iter().enumerate() {
                let shifted = if i >= d.gap_idx { *m } else { *m };
                if d.pointer_y < shifted {
                    idx = i;
                    break;
                }
            }
            if idx != d.gap_idx {
                d.glide = Some((d.gap_idx, idx, now));
                d.gap_idx = idx;
                ui.ctx().request_repaint();
            }
        }
        if drag_end || !ui.input(|i| i.pointer.primary_down()) {
            let d = app.drag.take().unwrap();
            commit_reorder(app, &d.fpr, d.gap_idx, &infos);
        }
    }

    if let Some((idx, fpr)) = pressed_row {
        app.show(&format!("chat:{fpr}"), |a| {
            a.open_chat(idx);
            a.nav.take().unwrap_or(crate::app::View::Welcome)
        });
    }
    if let Some(idx) = info_click {
        app.active = Some(idx);
        if let Some(c) = app.contacts.get(idx).cloned() {
            app.open_key_view(c);
        }
    }

    // ── the tabs, one per line, the open one inverted
    let my = side.max.y - 1.0 - 12.0 - 5.0 * LINE;
    let items: [(&str, &str); 5] = [
        ("add", "+ Add contact"),
        ("encrypt", "Sign / Encrypt"),
        ("decrypt", "Decrypt / Verify"),
        ("card", "Smartcard"),
        ("info", "Profile & keys"),
    ];
    for (i, (vid, label)) in items.iter().enumerate() {
        let lit = app.cur_id.as_deref() == Some(*vid);
        let r = ui::word(ui, Id::NULL.with(("tab", *vid)), x0 + 12.0, my + i as f32 * LINE, Word::new(label).lit(lit), live);
        if live && ui::pressed(ui, &r) {
            match *vid {
                "add" => app.show("add", |a| {
                    a.open_add();
                    a.nav.take().unwrap()
                }),
                "encrypt" => app.show("encrypt", |a| {
                    a.open_encrypt();
                    a.nav.take().unwrap()
                }),
                "decrypt" => app.show("decrypt", |a| {
                    a.open_decrypt();
                    a.nav.take().unwrap()
                }),
                "card" => app.show("card", |a| {
                    a.open_card();
                    a.nav.take().unwrap()
                }),
                _ => app.show("info", |a| {
                    a.open_info();
                    a.nav.take().unwrap()
                }),
            }
        }
    }
}

/// The floating copy of the dragged row, inside the list's viewport (call after the panels).
pub fn draw_ghost(app: &App, ui: &Ui) {
    let Some(d) = &app.drag else { return };
    let a = 237u8; // the Tk ghost window's -alpha 0.93
    let x = d.list.min.x + 12.0;
    let y = (d.pointer_y - d.off).max(d.list.min.y).min(d.list.min.y + (d.list.height() - d.gh).max(0.0));
    let r = Rect::from_min_size(pos2(x, y), vec2(196.0, d.gh));
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::NULL.with("ghost")));
    let g = col::GHOST;
    p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(g.r(), g.g(), g.b(), a));
    let t1 = Color32::from_rgba_unmultiplied(0xcc, 0xcc, 0xcc, a);
    let t3 = Color32::from_rgba_unmultiplied(0x76, 0x76, 0x76, a);
    let clip = r.shrink2(vec2(10.0, 0.0));
    let ctx = ui.ctx();
    for (i, (t, c)) in [(&d.name, t1), (&d.sub, t3)].into_iter().enumerate() {
        if t.is_empty() {
            continue;
        }
        let g = ui::galley(ctx, t, c);
        p.with_clip_rect(clip).galley(pos2(x + 11.0, ui::galley_y(&g, y + 10.0 + i as f32 * LINE)), g, c);
    }
}

/// Drops the dragged contact at slot `idx` among the other visible rows and saves the order
/// (search-hidden contacts go to the end, as before).
fn commit_reorder(app: &mut App, fpr: &str, idx: usize, infos: &[RowInfo]) {
    let active_fpr = app.active.and_then(|i| app.contacts.get(i)).map(|c| c.fpr.clone());
    let mut order: Vec<String> = infos.iter().filter(|r| r.fpr != fpr).map(|r| r.fpr.clone()).collect();
    let idx = idx.min(order.len());
    order.insert(idx, fpr.to_string());
    let mut new = Vec::new();
    for f in &order {
        if let Some(c) = app.contacts.iter().find(|c| &c.fpr == f) {
            new.push(c.clone());
        }
    }
    for c in &app.contacts {
        if !order.contains(&c.fpr) {
            new.push(c.clone());
        }
    }
    app.contacts = new;
    let fprs: Vec<String> = app.contacts.iter().map(|c| c.fpr.clone()).collect();
    store::set_contact_order(&fprs);
    if let Some(f) = active_fpr {
        app.active = app.contact_index(&f);
    }
}
