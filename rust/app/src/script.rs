//! Script mode, for testing the running app: `pgpm --script <file> --out <dir>` feeds the
//! commands in <file> to egui as real input (pointer, keys, text, dragged-in files), one at a
//! time, and prints the app's state on request. A `shot NAME` waits while a driver captures
//! the window (`<dir>/NAME.ready` -> `<dir>/NAME.done`).
//!
//! Commands: wait MS · move X Y · click X Y · press X Y · release X Y · type TEXT ·
//! paste TEXT · key NAME [cmd] · hoverfile X Y PATH · drop X Y PATH · shot NAME · state · quit

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Event, Key, Modifiers, PointerButton, pos2};

use crate::app::{App, View};

#[derive(Debug)]
struct TestFile(PathBuf);

impl egui::DroppedFile for TestFile {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

pub struct Script {
    cmds: VecDeque<String>,
    out: PathBuf,
    next_at: f64,
    waiting: Option<String>,
    /// a press to release on the next step
    release: Option<egui::Pos2>,
    /// a file being dragged over the window
    hover: Option<PathBuf>,
}

impl Script {
    pub fn load(path: &str, out: PathBuf) -> Script {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let cmds = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
        Script { cmds, out, next_at: 0.0, waiting: None, release: None, hover: None }
    }
}

fn key_of(name: &str) -> Option<Key> {
    Some(match name {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "A" => Key::A,
        "C" => Key::C,
        "V" => Key::V,
        "X" => Key::X,
        _ => return None,
    })
}

/// The app's state as JSON (one line on stdout).
fn state(app: &App) -> String {
    let view_text = match &app.view {
        View::Chat(c) => c.text.value().to_string(),
        View::Encrypt(e) => e.text.value().to_string(),
        View::Decrypt(d) => d.text.value().to_string(),
        View::Add(a) => a.text.value().to_string(),
        _ => String::new(),
    };
    let extra = match &app.view {
        View::Encrypt(e) => serde_json::json!({"pick": e.pick, "labels": e.labels, "pop": e.pop}),
        View::Decrypt(d) => serde_json::json!({"status": d.status.as_ref().map(|s| s.0.clone())}),
        View::Card(c) => serde_json::json!({"phase": match &c.phase {
            crate::card::Phase::Lone => "lone",
            crate::card::Phase::Detecting => "detecting",
            crate::card::Phase::Insert => "insert",
            crate::card::Phase::Info(_) => "info",
        }}),
        View::Info(p) => serde_json::json!({"generating": p.generating}),
        _ => serde_json::Value::Null,
    };
    serde_json::json!({
        "view": app.cur_id,
        "contacts": app.contacts.iter().map(|c| app.shown_name(c)).collect::<Vec<_>>(),
        "secret": app.secret.iter().map(|k| k.name.clone()).collect::<Vec<_>>(),
        "my_fpr": app.my_fpr,
        "active": app.active.and_then(|i| app.contacts.get(i)).map(|c| c.name.clone()),
        "search": app.search,
        "dialogs": app.dialogs.iter().map(|d| serde_json::json!({"title": d.title, "message": d.message})).collect::<Vec<_>>(),
        "text": view_text,
        "extra": extra,
        "update": app.update_tag,
        "card_present": app.card_present,
    })
    .to_string()
}

/// Runs the next command(s) due; called from `raw_input_hook`.
pub fn step(app: &mut App, raw: &mut egui::RawInput, now: f64) {
    let Some(s) = app.script.as_mut() else { return };
    if let Some(h) = &s.hover {
        raw.hovered_files.push(egui::HoveredFile { path: Some(h.clone()), mime: String::new() });
    }
    if let Some(name) = &s.waiting {
        if s.out.join(format!("{name}.done")).exists() {
            s.waiting = None;
        } else {
            return;
        }
    }
    if let Some(p) = s.release.take() {
        raw.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        s.next_at = now + 0.05;
        return;
    }
    if now < s.next_at {
        return;
    }
    let Some(cmd) = s.cmds.pop_front() else { return };
    s.next_at = now + 0.05;
    let mut parts = cmd.splitn(2, ' ');
    let op = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").to_string();
    let nums: Vec<f32> = rest.split_whitespace().take(2).filter_map(|v| v.parse().ok()).collect();
    let at = if nums.len() == 2 { Some(pos2(nums[0], nums[1])) } else { None };
    match op {
        "wait" => s.next_at = now + rest.trim().parse::<f64>().unwrap_or(100.0) / 1000.0,
        "move" => {
            if let Some(p) = at {
                raw.events.push(Event::PointerMoved(p));
            }
        }
        "click" | "press" => {
            if let Some(p) = at {
                raw.events.push(Event::PointerMoved(p));
                raw.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                if op == "click" {
                    s.release = Some(p);
                }
            }
        }
        "release" => {
            if let Some(p) = at {
                raw.events.push(Event::PointerMoved(p));
                raw.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            }
        }
        "scroll" => {
            // scroll X Y DY: the wheel over (X, Y)
            let v: Vec<f32> = rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if v.len() == 3 {
                raw.events.push(Event::PointerMoved(pos2(v[0], v[1])));
                raw.events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, v[2]),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                    source: egui::MouseWheelSource::Wheel,
                });
            }
        }
        "type" => raw.events.push(Event::Text(rest)),
        "paste" => raw.events.push(Event::Paste(rest.replace("\\n", "\n"))),
        "key" => {
            let mut it = rest.split_whitespace();
            if let Some(k) = it.next().and_then(key_of) {
                let m = if it.next() == Some("cmd") { Modifiers::MAC_CMD | Modifiers::COMMAND } else { Modifiers::NONE };
                raw.events.push(Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: m });
                raw.events.push(Event::Key { key: k, physical_key: Some(k), pressed: false, repeat: false, modifiers: m });
            }
        }
        "hoverfile" | "drop" => {
            let path: String = rest.split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
            if let Some(p) = at {
                app.test_pointer = Some(p);
            }
            let s = app.script.as_mut().unwrap();
            if op == "hoverfile" {
                s.hover = Some(PathBuf::from(&path));
            } else {
                s.hover = None;
                raw.dropped_files.push(Arc::new(TestFile(PathBuf::from(path))));
            }
        }
        "shot" => {
            let name = rest.trim().to_string();
            let _ = std::fs::write(s.out.join(format!("{name}.ready")), b"ready");
            s.waiting = Some(name);
        }
        "state" => println!("STATE {}", state(app)),
        "quit" => {
            println!("QUIT");
            std::process::exit(0);
        }
        other => println!("UNKNOWN {other}"),
    }
}
