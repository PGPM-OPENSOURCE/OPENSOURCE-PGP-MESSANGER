//! Snapshot mode, for comparing screens with the Tk app's captures:
//! `pgpm --snap <scenario> --out <dir>` opens one screen in a known state, writes
//! `<dir>/<scenario>.ready` once it has been drawn, and stays up until it is captured
//! (a driver grabs the window) or for 20 seconds.

use std::path::PathBuf;

use egui::pos2;
use pgpm_core::gpg::{self, CardInfo};

use crate::app::{App, View};
use crate::card::{CardPanel, Phase};

pub struct Snap {
    pub scenario: String,
    pub out: PathBuf,
    /// a pointer position to hold (hover states)
    pub hover: Option<egui::Pos2>,
    frames: u32,
    started: Option<f64>,
}

impl Snap {
    pub fn new(scenario: String, out: PathBuf) -> Snap {
        Snap { scenario, out, hover: None, frames: 0, started: None }
    }
}

fn idx(app: &App, name: &str) -> Option<usize> {
    app.contacts.iter().position(|c| c.name.starts_with(name))
}

fn card() -> CardInfo {
    let mut c = CardInfo { present: true, ..Default::default() };
    for (k, v) in [
        ("serial", "0651E788"),
        ("name", "Ipecac"),
        ("version", "3.4"),
        ("reader", "Nitrokey Nitrokey 3"),
        ("sig_fpr", "0D3E670CBB5B237FCA3DECD1AD0405C434EC0254"),
        ("enc_fpr", "407F1A54B0D93D885CC566349400CB5773EAD5AC"),
        ("auth_fpr", ""),
        ("pin_retries", "3 0 3"),
        ("url", ""),
    ] {
        c.fields.insert(k.into(), v.into());
    }
    c
}

/// Puts the app in the scenario's state (the same steps the Tk captures took).
pub fn setup(app: &mut App, s: &Snap) {
    let sc = s.scenario.as_str();
    let bob = idx(app, "Bob");
    if !matches!(sc, "welcome" | "hover-menu" | "row-hover") {
        app.active = bob;
    }
    match sc {
        "welcome" | "hover-menu" | "row-hover" => {}
        "chat" => {
            if let Some(i) = bob {
                app.open_chat(i);
            }
        }
        "encrypt" | "encrypt-pick" | "min-encrypt" => {
            app.open_encrypt();
            if sc == "encrypt-pick" {
                if let Some(View::Encrypt(e)) = app.nav.as_mut() {
                    e.pop = true;
                }
            }
        }
        "decrypt" | "decrypt-verified" => {
            app.open_decrypt();
            // decrypt-verified: the message is decrypted on the first frame (fonts are loaded)
        }
        "card-lone" | "card-insert" | "card-info" => {
            let serial = app.next_serial();
            let phase = match sc {
                "card-insert" => Phase::Insert,
                "card-info" => Phase::Info(card()),
                _ => Phase::Lone,
            };
            app.go(View::Card(CardPanel { serial, phase, busy_use: false, vals: Vec::new() }));
        }
        "profile" | "profile-bottom" => app.open_info(),
        "add" => app.open_add(),
        "key-dave" | "key-bob" | "dialog-nick" | "dialog-ask" | "update" => {
            let who = if sc == "key-dave" { "Dave" } else { "Bob" };
            if let Some(i) = idx(app, who) {
                app.active = Some(i);
                let c = app.contacts[i].clone();
                app.open_key_view(c);
            }
            match sc {
                "dialog-nick" => app.ask_string(
                    "Nickname",
                    "A nickname for Bob Example, shown in your contacts. Leave it empty to remove it.",
                    "",
                    |_, _| {},
                ),
                "dialog-ask" => app.ask("Delete key", "Remove Bob Example from the keyring?\nThis cannot be undone.", |_, _| {}),
                "update" => {
                    crate::fonts::install(&app.ctx, true);
                    app.update_tag = Some("9.9.9".into());
                }
                _ => {}
            }
        }
        "dialog-warn" => {
            if let Some(i) = bob {
                let c = app.contacts[i].clone();
                app.open_key_view(c);
            }
            app.warn("Can't encrypt", "Dave Old's key has expired, so messages can't be encrypted to it.\n\nImport a current public key for them.");
        }
        "search" => {
            if let Some(i) = bob {
                let c = app.contacts[i].clone();
                app.open_key_view(c);
            }
            app.search = "car".into();
            app.ctx.memory_mut(|m| m.request_focus(egui::Id::NULL.with("search")));
        }
        _ => {}
    }
    if let Some(v) = app.nav.take() {
        app.cur_id = v.id();
        app.view = v;
    }
}

/// The decrypt status's signer, fitted to the default window's status line.
pub fn fit(ctx: &egui::Context, signer: &str) -> String {
    let room = 262.0 - 20.0;
    let prefix = " valid signature from ";
    let name = signer.split('<').next().unwrap_or("").trim();
    for cand in [signer, name] {
        if crate::ui::measure(ctx, &format!("{prefix}{cand}")) <= room {
            return cand.into();
        }
    }
    let mut n: Vec<char> = name.chars().collect();
    while n.len() > 1 && crate::ui::measure(ctx, &format!("{prefix}{}…", n.iter().collect::<String>())) > room {
        n.pop();
    }
    format!("{}…", n.iter().collect::<String>().trim_end())
}

/// Hover positions (window points) of the scenarios that capture one.
pub fn hover_for(scenario: &str) -> Option<egui::Pos2> {
    match scenario {
        "hover-menu" => Some(pos2(40.0, 430.0)),
        "row-hover" => Some(pos2(60.0, 130.0)),
        _ => None,
    }
}

/// Counts frames; once the screen has settled, says it's ready (and scrolls / resizes first).
pub fn tick(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.snap.as_mut() else { return };
    s.frames += 1;
    let (frames, scenario) = (s.frames, s.scenario.clone());
    let now = ctx.input(|i| i.time);
    let t0 = *s.started.get_or_insert(now);
    let out = s.out.clone();
    if frames == 2 && scenario == "decrypt-verified" {
        let signed = gpg::sign("Signed hello from Alice.", app.my_fpr.as_deref()).unwrap_or_default();
        if let (Ok((text, signer)), View::Decrypt(d)) = (gpg::decrypt(&signed), &mut app.view) {
            d.text.set(text);
            let sg = signer.unwrap_or_default();
            d.status = Some((format!("✓ valid signature from {}", fit(ctx, &sg)), crate::ui::col::GN, f64::INFINITY));
            let id = egui::Id::NULL.with(("decrypt", d.serial)).with("text");
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }
    if frames == 2 && scenario == "min-encrypt" {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 400.0)));
    }
    if frames == 3 && scenario == "profile-bottom" {
        ctx.data_mut(|d| d.insert_temp(egui::Id::NULL.with("snap-scroll-bottom"), true));
    }
    if frames == 12 {
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(out.join(format!("{scenario}.ready")), b"ready");
    }
    if now - t0 > 20.0 {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
}
