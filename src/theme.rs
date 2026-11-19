//! Kolibri design system: color palettes, typography, spacing, sizes, radii,
//! and the global egui `Style` built from them.
//!
//! Screens never hardcode colors or sizes; they read them from here
//! (`theme::palette(ui)`, `space::*`, `radius::*`, `size::*`, `font::*`).

use eframe::egui::{
    self, style::ScrollStyle, vec2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily,
    FontId, Margin, Shadow, Stroke, TextStyle, Theme, Visuals,
};
use std::sync::Arc;

// ---------------------------------------------------------------- palette

/// Semantic colors. Every screen uses these names, never raw hex values.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Sidebar / navigation background.
    pub bg_sidebar: Color32,
    /// Main workspace background.
    pub bg_app: Color32,
    /// Inputs, cards, code areas.
    pub bg_surface: Color32,
    /// Menus, popups, dialogs.
    pub bg_elevated: Color32,
    pub bg_hover: Color32,
    /// Pressed controls and the selected row / tab.
    pub bg_active: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text drawn on top of `accent` / `danger` fills.
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
    pub purple: Color32,
    pub neutral: Color32,
    pub shadow: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
    dark: true,
    bg_sidebar: rgb(0x111214),
    bg_app: rgb(0x16171A),
    bg_surface: rgb(0x1C1D21),
    bg_elevated: rgb(0x212227),
    bg_hover: rgb(0x24252A),
    bg_active: rgb(0x2B2C32),
    border: rgb(0x26272C),
    border_strong: rgb(0x34353C),
    text: rgb(0xE6E6E9),
    text_muted: rgb(0x9D9EA6),
    text_faint: rgb(0x6C6D76),
    accent: rgb(0x6B74F0),
    accent_hover: rgb(0x7E86F4),
    on_accent: rgb(0xFFFFFF),
    success: rgb(0x4CC38A),
    warning: rgb(0xF0B44C),
    danger: rgb(0xEF5F6B),
    info: rgb(0x5AA9FA),
    purple: rgb(0xB18CF5),
    neutral: rgb(0x9AA0AA),
    shadow: Color32::from_black_alpha(110),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg_sidebar: rgb(0xF6F6F8),
    bg_app: rgb(0xFFFFFF),
    bg_surface: rgb(0xFBFBFC),
    bg_elevated: rgb(0xFFFFFF),
    bg_hover: rgb(0xEFEFF3),
    bg_active: rgb(0xE7E8EE),
    border: rgb(0xE5E6EB),
    border_strong: rgb(0xD3D4DB),
    text: rgb(0x1C1D21),
    text_muted: rgb(0x5E5F68),
    text_faint: rgb(0x8E8F99),
    accent: rgb(0x5259E6),
    accent_hover: rgb(0x454CD9),
    on_accent: rgb(0xFFFFFF),
    success: rgb(0x1E9E62),
    warning: rgb(0xB7791F),
    danger: rgb(0xD9394A),
    info: rgb(0x2878D4),
    purple: rgb(0x7C4DDB),
    neutral: rgb(0x6B7280),
    shadow: Color32::from_black_alpha(40),
};

/// The palette for the current theme of `ui`.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    palette_for(ui.visuals().dark_mode)
}

pub fn palette_for(dark: bool) -> &'static Palette {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

/// Linear blend of two opaque colors (`t` = 0 gives `a`, 1 gives `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

impl Palette {
    /// Soft tinted background for badges and callouts in `color`.
    pub fn tint(&self, color: Color32) -> Color32 {
        mix(self.bg_app, color, if self.dark { 0.14 } else { 0.10 })
    }

    /// Border for a tinted element.
    pub fn tint_border(&self, color: Color32) -> Color32 {
        mix(self.bg_app, color, if self.dark { 0.32 } else { 0.30 })
    }

    pub fn method(&self, m: crate::model::Method) -> Color32 {
        use crate::model::Method::*;
        match m {
            GET => self.success,
            POST => self.warning,
            PUT => self.info,
            PATCH => self.purple,
            DELETE => self.danger,
            HEAD | OPTIONS => self.neutral,
        }
    }

    pub fn status(&self, code: u16) -> Color32 {
        match code {
            200..=299 => self.success,
            300..=399 => self.info,
            400..=499 => self.warning,
            _ => self.danger,
        }
    }
}

// ---------------------------------------------------------------- tokens

/// Spacing scale (points).
pub mod space {
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Corner radii.
pub mod radius {
    pub const SM: u8 = 4;
    pub const MD: u8 = 6;
    pub const LG: u8 = 8;
    pub const XL: u8 = 12;
}

/// Component sizes.
pub mod size {
    /// Standard control height (buttons, inputs, tabs).
    pub const CONTROL: f32 = 28.0;
    /// Large control height (URL bar, primary actions).
    pub const CONTROL_LG: f32 = 34.0;
    /// Compact control height (icon buttons, small buttons).
    pub const CONTROL_SM: f32 = 24.0;
    /// Sidebar tree row.
    pub const ROW: f32 = 28.0;
    /// Table row (params, headers).
    pub const TABLE_ROW: f32 = 30.0;
    pub const ICON: f32 = 14.0;
    pub const ICON_SM: f32 = 12.0;
    pub const SIDEBAR_DEFAULT: f32 = 264.0;
    pub const SIDEBAR_MIN: f32 = 200.0;
    pub const SIDEBAR_MAX: f32 = 440.0;
}

/// Typography. Inter for the interface, JetBrains Mono for code and data.
pub mod font {
    use super::*;

    pub const MEDIUM: &str = "inter-medium";
    pub const SEMIBOLD: &str = "inter-semibold";

    pub fn medium_family() -> FontFamily {
        FontFamily::Name(MEDIUM.into())
    }
    pub fn semibold_family() -> FontFamily {
        FontFamily::Name(SEMIBOLD.into())
    }

    /// Regular interface text.
    pub fn body() -> FontId {
        FontId::proportional(13.0)
    }
    /// Labels, buttons, tab titles.
    pub fn label() -> FontId {
        FontId::new(13.0, medium_family())
    }
    /// Secondary text and captions.
    pub fn small() -> FontId {
        FontId::proportional(12.0)
    }
    pub fn small_medium() -> FontId {
        FontId::new(12.0, medium_family())
    }
    /// Uppercase section headers.
    pub fn overline() -> FontId {
        FontId::new(10.5, semibold_family())
    }
    /// Screen titles.
    pub fn title() -> FontId {
        FontId::new(15.0, semibold_family())
    }
    /// Dialog and empty-state titles.
    pub fn heading() -> FontId {
        FontId::new(14.0, semibold_family())
    }
    /// Large numbers in metric cards.
    pub fn metric() -> FontId {
        FontId::new(20.0, semibold_family())
    }
    pub fn mono() -> FontId {
        FontId::monospace(12.5)
    }
    pub fn mono_small() -> FontId {
        FontId::monospace(10.5)
    }
}

// ---------------------------------------------------------------- fonts

macro_rules! font_file {
    ($name:literal) => {
        include_bytes!(concat!("../assets/fonts/", $name, ".ttf"))
    };
}

fn font_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        name.to_owned()
    };
    // Each weight is split into Latin, Latin Extended and Cyrillic files.
    let inter_400 = [
        add("inter-400-latin", font_file!("inter-latin-400")),
        add("inter-400-latin-ext", font_file!("inter-latin-ext-400")),
        add("inter-400-cyrillic", font_file!("inter-cyrillic-400")),
    ];
    let inter_500 = [
        add("inter-500-latin", font_file!("inter-latin-500")),
        add("inter-500-latin-ext", font_file!("inter-latin-ext-500")),
        add("inter-500-cyrillic", font_file!("inter-cyrillic-500")),
    ];
    let inter_600 = [
        add("inter-600-latin", font_file!("inter-latin-600")),
        add("inter-600-latin-ext", font_file!("inter-latin-ext-600")),
        add("inter-600-cyrillic", font_file!("inter-cyrillic-600")),
    ];
    let mono = [
        add("jbmono-latin", font_file!("jetbrains-mono-latin-400")),
        add(
            "jbmono-latin-ext",
            font_file!("jetbrains-mono-latin-ext-400"),
        ),
        add("jbmono-cyrillic", font_file!("jetbrains-mono-cyrillic-400")),
    ];

    // egui's built-in fonts stay as fallbacks (symbols, emoji, other scripts).
    let prop_fallback = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mono_fallback = defs
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let chain =
        |first: &[String], rest: &[String]| first.iter().chain(rest).cloned().collect::<Vec<_>>();

    defs.families
        .insert(FontFamily::Proportional, chain(&inter_400, &prop_fallback));
    defs.families
        .insert(font::medium_family(), chain(&inter_500, &prop_fallback));
    defs.families
        .insert(font::semibold_family(), chain(&inter_600, &prop_fallback));
    defs.families
        .insert(FontFamily::Monospace, chain(&mono, &mono_fallback));
    defs
}

// ---------------------------------------------------------------- style

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let cr = CornerRadius::same(radius::MD);

    v.panel_fill = p.bg_app;
    v.window_fill = p.bg_elevated;
    v.window_stroke = Stroke::new(1.0, p.border_strong);
    v.window_corner_radius = CornerRadius::same(radius::XL);
    v.menu_corner_radius = CornerRadius::same(radius::LG);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: p.shadow,
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: p.shadow,
    };
    v.window_highlight_topmost = false;

    v.extreme_bg_color = p.bg_surface;
    v.text_edit_bg_color = Some(p.bg_surface);
    v.code_bg_color = p.bg_surface;
    v.faint_bg_color = mix(p.bg_app, p.text, 0.025);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.weak_text_color = Some(p.text_faint);
    v.override_text_color = None;

    v.selection.bg_fill = mix(p.bg_app, p.accent, if p.dark { 0.42 } else { 0.28 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg_app;
    w.noninteractive.weak_bg_fill = p.bg_app;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_muted);
    w.noninteractive.corner_radius = cr;

    w.inactive.bg_fill = p.bg_surface;
    w.inactive.weak_bg_fill = p.bg_surface;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = cr;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.bg_hover;
    w.hovered.weak_bg_fill = p.bg_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, mix(p.border_strong, p.text_faint, 0.35));
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = cr;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.bg_active;
    w.active.weak_bg_fill = p.bg_active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = cr;
    w.active.expansion = 0.0;

    w.open = w.active;
    w.open.bg_stroke = Stroke::new(1.0, p.border_strong);

    v.striped = false;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;
    v.slider_trailing_fill = true;
    v.resize_corner_size = 10.0;
    v
}

fn style_for(style: &mut egui::Style, p: &Palette) {
    style.visuals = visuals(p);

    let s = &mut style.spacing;
    s.item_spacing = vec2(space::SM, space::SM);
    s.button_padding = vec2(10.0, 4.0);
    s.interact_size = vec2(40.0, size::CONTROL);
    s.menu_margin = Margin::same(6);
    s.window_margin = Margin::same(20);
    s.icon_width = 16.0;
    s.icon_width_inner = 8.0;
    s.icon_spacing = 6.0;
    s.combo_height = 300.0;
    s.menu_width = 200.0;
    s.tooltip_width = 320.0;
    s.scroll = ScrollStyle::thin();
    s.scroll.floating = true;

    style.text_styles = [
        (TextStyle::Small, font::small()),
        (TextStyle::Body, font::body()),
        (TextStyle::Button, font::label()),
        (TextStyle::Heading, font::title()),
        (TextStyle::Monospace, font::mono()),
    ]
    .into();

    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.35;
    style.animation_time = 0.12;
}

/// Install fonts and the dark and light styles. Call once at startup.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    ctx.style_mut_of(Theme::Dark, |s| style_for(s, &DARK));
    ctx.style_mut_of(Theme::Light, |s| style_for(s, &LIGHT));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_endpoints() {
        let a = rgb(0x000000);
        let b = rgb(0xFFFFFF);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 0.5), Color32::from_rgb(128, 128, 128));
    }

    #[test]
    fn fonts_install() {
        let ctx = egui::Context::default();
        install(&ctx);
        let defs = font_definitions();
        assert!(defs.families[&font::semibold_family()][0].starts_with("inter-600"));
        assert!(defs.families[&FontFamily::Monospace][0].starts_with("jbmono"));
    }
}
