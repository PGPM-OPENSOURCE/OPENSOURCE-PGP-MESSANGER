//! PGPM — OpenPGP keys and messages, as a native app written in Rust. The same app as the
//! Tk one (the console look, GnuPG as the engine, the same settings in ~/.pgp_messenger).

mod app;
mod card;
mod dialog;
mod fonts;
#[cfg(target_os = "macos")]
mod macos;
mod panels;
mod profile;
mod script;
mod sidebar;
mod snap;
mod ui;

/// The pointer in window points while a file is dragged over the window.
pub fn macos_pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(target_os = "macos")]
    {
        macos::pointer_in_window(ctx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        ctx.input(|i| i.pointer.latest_pos())
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let snap = arg("--snap").map(|s| {
        let mut sn = snap::Snap::new(s, arg("--out").unwrap_or_else(|| ".".into()).into());
        sn.hover = snap::hover_for(&sn.scenario);
        sn
    });

    let script = arg("--script").map(|f| script::Script::load(&f, arg("--out").unwrap_or_else(|| ".".into()).into()));

    let icon = image::load_from_memory(include_bytes!("../../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([920.0, 510.0])
        .with_min_inner_size([640.0, 400.0])
        .with_title("PGPM");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("PGPM", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, snap, script)))))
}
