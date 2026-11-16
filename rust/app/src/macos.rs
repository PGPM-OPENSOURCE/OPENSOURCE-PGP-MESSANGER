//! macOS: the menu bar as the Tk app had it (About PGPM, Edit, Window), and the pointer's
//! position while a file is dragged over the window (winit reports the files, not where).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSMenu, NSMenuItem, NSScreen};
use objc2_foundation::NSString;

static MENU_EVENTS: Mutex<Vec<egui::Event>> = Mutex::new(Vec::new());
static ABOUT: AtomicBool = AtomicBool::new(false);
static CTX: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

fn push(e: egui::Event) {
    if let Ok(mut q) = MENU_EVENTS.lock() {
        q.push(e);
    }
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

/// Edit-menu actions queued since the last frame (fed in through `raw_input_hook`).
pub fn take_menu_events() -> Vec<egui::Event> {
    MENU_EVENTS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// "About PGPM" was chosen (once).
pub fn take_about() -> bool {
    ABOUT.swap(false, Ordering::Relaxed)
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and MenuTarget does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PGPMMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(pgpmAbout:))]
        fn about(&self, _sender: Option<&AnyObject>) {
            ABOUT.store(true, Ordering::Relaxed);
            if let Some(c) = CTX.get() {
                c.request_repaint();
            }
        }
        #[unsafe(method(pgpmCut:))]
        fn cut(&self, _sender: Option<&AnyObject>) {
            push(egui::Event::Cut);
        }
        #[unsafe(method(pgpmCopy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            push(egui::Event::Copy);
        }
        #[unsafe(method(pgpmPaste:))]
        fn paste(&self, _sender: Option<&AnyObject>) {
            let m = egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND;
            push(egui::Event::Key { key: egui::Key::V, physical_key: Some(egui::Key::V), pressed: true, repeat: false, modifiers: m });
            push(egui::Event::Key { key: egui::Key::V, physical_key: Some(egui::Key::V), pressed: false, repeat: false, modifiers: m });
        }
        #[unsafe(method(pgpmSelectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) {
            let m = egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND;
            push(egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true, repeat: false, modifiers: m });
            push(egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: false, repeat: false, modifiers: m });
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn item(mtm: MainThreadMarker, title: &str, action: Option<objc2::runtime::Sel>, key: &str) -> Retained<NSMenuItem> {
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), action, &NSString::from_str(key)) }
}

/// The menu bar and the dark title bar (also in light mode).
pub fn install(ctx: &egui::Context) {
    let _ = CTX.set(ctx.clone());
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let target = MenuTarget::new(mtm);
    let main = NSMenu::new(mtm);

    let app_menu = NSMenu::new(mtm);
    let about = item(mtm, "About PGPM", Some(sel!(pgpmAbout:)), "");
    unsafe { about.setTarget(Some(&target)) };
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(mtm, "Hide PGPM", Some(sel!(hide:)), "h"));
    let others = item(mtm, "Hide Others", Some(sel!(hideOtherApplications:)), "h");
    others.setKeyEquivalentModifierMask(NSEventModifierFlags::Option | NSEventModifierFlags::Command);
    app_menu.addItem(&others);
    app_menu.addItem(&item(mtm, "Show All", Some(sel!(unhideAllApplications:)), ""));
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(mtm, "Quit PGPM", Some(sel!(terminate:)), "q"));
    let app_item = NSMenuItem::new(mtm);
    app_item.setSubmenu(Some(&app_menu));
    main.addItem(&app_item);

    let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Edit"));
    for (title, action, k) in [
        ("Cut", sel!(pgpmCut:), "x"),
        ("Copy", sel!(pgpmCopy:), "c"),
        ("Paste", sel!(pgpmPaste:), "v"),
        ("Select All", sel!(pgpmSelectAll:), "a"),
    ] {
        let it = item(mtm, title, Some(action), k);
        unsafe { it.setTarget(Some(&target)) };
        edit.addItem(&it);
    }
    let edit_item = NSMenuItem::new(mtm);
    edit_item.setSubmenu(Some(&edit));
    main.addItem(&edit_item);

    let window = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Window"));
    window.addItem(&item(mtm, "Minimize", Some(sel!(performMiniaturize:)), "m"));
    window.addItem(&item(mtm, "Zoom", Some(sel!(performZoom:)), ""));
    let window_item = NSMenuItem::new(mtm);
    window_item.setSubmenu(Some(&window));
    main.addItem(&window_item);

    app.setMainMenu(Some(&main));
    app.setWindowsMenu(Some(&window));
    // menu items hold their target weakly: it lives as long as the app
    std::mem::forget(target);
}

/// The pointer in window points (top-left origin), from AppKit (it keeps moving while a
/// file drag runs, when the window gets no pointer events).
pub fn pointer_in_window(ctx: &egui::Context) -> Option<egui::Pos2> {
    let mtm = MainThreadMarker::new()?;
    let inner = ctx.input(|i| i.viewport().inner_rect)?;
    let loc = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let main_h = screens.firstObject()?.frame().size.height;
    Some(egui::pos2(loc.x as f32 - inner.min.x, (main_h - loc.y) as f32 - inner.min.y))
}
