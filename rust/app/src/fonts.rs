//! Fonts: Cascadia Mono (PGPM's console font, the repo's fonts/ with its OFL licence) first, then
//! the macOS fallbacks CoreText would use for the few glyphs it lacks (as Tk draws them).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

pub const CASCADIA: &[u8] = include_bytes!("../../../fonts/CascadiaMono-Regular.ttf");

/// Cascadia Mono metrics (units per em 2048).
pub const UPEM: f32 = 2048.0;
pub const ASCENT: f32 = 1900.0;
pub const DESCENT: f32 = 480.0;

/// Cascadia Mono, then what CoreText falls back to from it on macOS: Menlo (✕), then Apple
/// Symbols. With `cjk` (only needed for the update banner's ⬆), Hiragino Sans W3 before Apple
/// Symbols, as CoreText picks it.
pub fn install(ctx: &egui::Context, cjk: bool) {
    let mut defs = FontDefinitions::default();
    defs.font_data.insert("cascadia".into(), Arc::new(FontData::from_static(CASCADIA)));
    let mut chain = vec!["cascadia".to_string()];
    let mut fallbacks = vec![("menlo", "/System/Library/Fonts/Menlo.ttc")];
    if cjk {
        fallbacks.push(("hiragino", "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"));
    }
    fallbacks.push(("apple-symbols", "/System/Library/Fonts/Apple Symbols.ttf"));
    for (name, path) in fallbacks {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fd = FontData::from_owned(bytes);
            if name == "hiragino" {
                // on Cascadia's baseline, where CoreText puts it
                fd = fd.tweak(egui::FontTweak { y_offset: 3.5, ..Default::default() });
            }
            defs.font_data.insert(name.into(), Arc::new(fd));
            chain.push(name.into());
        }
    }
    for fam in [FontFamily::Monospace, FontFamily::Proportional] {
        let list = defs.families.entry(fam).or_default();
        for (i, n) in chain.iter().enumerate() {
            list.insert(i, n.clone());
        }
    }
    ctx.set_fonts(defs);
}
