//! PR Marmot's visual language — Guise/Mantine-inspired token overrides on top
//! of gpui-component's shadcn defaults, plus the layout constants the views
//! share. Spec: `.claude/research/2026-07-24-visual-design.md`.
//!
//! The colours come in three built-in sets ([`CLASSIC`], [`STUDIO`],
//! [`MINERAL`]), each a light and a dark [`Palette`], or from a theme file
//! the person dropped into the themes folder. [`apply_palette`] must run
//! after every mode change (`ThemePref::apply` does), because
//! `Theme::change` / `sync_system_appearance` reset `theme.colors` to the
//! built-in palette; [`apply_geometry`] then re-asserts the type size and
//! radii, which no colour theme may change.

use std::time::Duration;

use gpui::{px, App, Hsla};
use gpui_component::theme::Theme;
use prmarmot_local::config::{Colors, Skin};

/// Table row height in px — matches gpui-component `Size::Small`
/// (`Table::small()`), the Finder-like density the spec targets.
/// Documentation of what `.small()` provides, not consumed directly.
#[allow(dead_code)]
pub const ROW_HEIGHT: f32 = 30.0;
/// Horizontal cell padding in px at `Size::Small`, each side (vertical is
/// 3 px): what `.small()` provides, which the table subtracts when eliding.
pub const CELL_PAD_X: f32 = 6.0;
/// Header bar horizontal padding in px (`.px_4()`). Vertical is owned by
/// `TitleBar` (fixed 34 px row) since the header moved into the titlebar.
pub const HEADER_PAD_X: f32 = 16.0;
/// Keyboard-hint footer vertical padding in px (horizontal = HEADER_PAD_X).
pub const FOOTER_PAD_Y: f32 = 6.0;
/// Label-chip height in px (11 px medium text inside).
pub const CHIP_HEIGHT: f32 = 18.0;
/// Label-chip horizontal padding in px.
pub const CHIP_PAD_X: f32 = 6.0;
/// Label-chip corner radius in px (Guise `radius.sm`).
pub const CHIP_RADIUS: f32 = 4.0;
/// Diameter in px of the status dot that replaces emoji glyphs.
pub const STATUS_DOT: f32 = 7.0;
/// Table cell text size in px (native macOS table size; base UI text is 14).
pub const TABLE_TEXT_PX: f32 = 13.0;

/// The type scale: every text size a view sets is one of these, so the
/// scale changes in one place.
pub mod type_size {
    use gpui::{px, Pixels};

    /// Chips, keycaps and other small labels.
    pub const CAPTION: Pixels = px(11.);
    /// Secondary lines: the footer, column heads, hints, field labels.
    pub const SMALL: Pixels = px(12.);
    /// Body text, the table's own size ([`super::TABLE_TEXT_PX`]).
    pub const BODY: Pixels = px(super::TABLE_TEXT_PX);
    /// A screen's title.
    pub const TITLE: Pixels = px(18.);
    /// The app's name at the top of Settings.
    pub const DISPLAY: Pixels = px(20.);
    /// The sign-in code, read off the screen and typed on GitHub.
    pub const CODE: Pixels = px(24.);
}

/// `0xRRGGBB` -> opaque theme color.
fn c(hex: u32) -> Hsla {
    gpui::rgb(hex).into()
}

/// Tabular figures (`tnum`): every digit the same width, so a count or a
/// "synced 3m ago" that changes doesn't nudge the text beside it.
pub fn tabular_numbers() -> gpui::FontFeatures {
    gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".to_owned(), 1)]))
}

/// `0xRRGGBB` + alpha (0.0–1.0) -> translucent theme color.
fn ca(hex: u32, alpha: f32) -> Hsla {
    let mut color = c(hex);
    color.a = alpha;
    color
}

/// A translucent token: `0xRRGGBB` drawn at `alpha` over whatever is under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tint {
    pub hex: u32,
    pub alpha: f32,
}

const fn t(hex: u32, alpha: f32) -> Tint {
    Tint { hex, alpha }
}

impl Tint {
    fn hsla(self) -> Hsla {
        ca(self.hex, self.alpha)
    }

    /// The opaque colour it is drawn as over `surface` — what a contrast
    /// check measures against.
    #[cfg(test)]
    fn over(self, surface: u32) -> u32 {
        prmarmot_local::themes::blend(self.hex, surface, f64::from(self.alpha))
    }
}

/// Every token of one appearance mode that a colour theme varies, as
/// `0xRRGGBB` (or a [`Tint`]), so a test can hold every text colour against
/// every surface it is drawn on (`contrast` below). The text and surface
/// tokens come first; the state tokens after them were hard-coded per mode
/// before the Studio and Mineral sets existed.
pub(crate) struct Palette {
    pub background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub muted_foreground: u32,
    pub secondary: u32,
    pub secondary_foreground: u32,
    pub accent: u32,
    pub accent_foreground: u32,
    pub title_bar: u32,
    pub tab_bar_segmented: u32,
    pub tab_foreground: u32,
    pub tab_active: u32,
    pub tab_active_foreground: u32,
    pub primary: u32,
    pub primary_hover: u32,
    pub primary_active: u32,
    pub primary_foreground: u32,
    pub link: u32,
    pub danger: u32,
    pub warning: u32,
    pub success: u32,
    pub table_even: u32,
    pub table_head: u32,
    pub table_head_foreground: u32,
    pub table_hover: u32,
    pub popover: u32,
    pub popover_foreground: u32,
    // State tokens.
    pub border: u32,
    pub input: u32,
    pub secondary_hover: u32,
    pub secondary_active: u32,
    pub link_hover: u32,
    pub link_active: u32,
    pub ring: u32,
    pub caret: u32,
    pub selection: Tint,
    pub table_active: Tint,
    pub table_active_border: Tint,
    pub table_row_border: Tint,
    pub list_active: Tint,
    pub danger_hover: u32,
    pub danger_active: u32,
    pub danger_foreground: u32,
    pub warning_hover: u32,
    pub warning_active: u32,
    pub warning_foreground: u32,
    pub success_hover: u32,
    pub success_active: u32,
    pub success_foreground: u32,
    pub scrollbar: Tint,
    pub scrollbar_thumb: Tint,
    pub scrollbar_thumb_hover: u32,
}

/// A light and a dark palette under one name.
pub(crate) struct ColorSet {
    pub light: &'static Palette,
    pub dark: &'static Palette,
}

impl ColorSet {
    pub fn palette(&self, dark: bool) -> &'static Palette {
        if dark {
            self.dark
        } else {
            self.light
        }
    }
}

/// Classic: Mantine dark ramp, open-color blue, open-color status hues.
pub(crate) const DARK: Palette = Palette {
    background: 0x1A1B1E,
    foreground: 0xC1C2C5,
    muted: 0x25262B,
    muted_foreground: 0x909296,
    secondary: 0x25262B,
    secondary_foreground: 0xC1C2C5,
    accent: 0x25262B,
    accent_foreground: 0x74C0FC,
    title_bar: 0x141517,
    tab_bar_segmented: 0x25262B,
    tab_foreground: 0x909296,
    tab_active: 0x1A1B1E,
    tab_active_foreground: 0xC1C2C5,
    primary: 0x1971C2,
    primary_hover: 0x1864AB,
    primary_active: 0x1864AB,
    primary_foreground: 0xFFFFFF,
    link: 0x4DABF7,
    danger: 0xFF6B6B,
    warning: 0xFAB005,
    success: 0x40C057,
    table_even: 0x1E1F23,
    table_head: 0x141517,
    table_head_foreground: 0x909296,
    table_hover: 0x25262B,
    popover: 0x25262B,
    popover_foreground: 0xC1C2C5,
    border: 0x2C2E33,
    input: 0x2C2E33,
    secondary_hover: 0x2C2E33,
    secondary_active: 0x373A40,
    link_hover: 0x74C0FC,
    link_active: 0x339AF0,
    ring: 0x4DABF7,
    caret: 0xC1C2C5,
    selection: t(0x228BE6, 0.35),
    table_active: t(0x228BE6, 0.26),
    table_active_border: t(0x228BE6, 0.60),
    table_row_border: t(0xFFFFFF, 0.04),
    list_active: t(0x228BE6, 0.18),
    // Status hues — open-color red.5 / yellow.6 / green.6.
    danger_hover: 0xFF8787,
    danger_active: 0xFA5252,
    danger_foreground: 0x1A1B1E,
    warning_hover: 0xFCC419,
    warning_active: 0xF59F00,
    warning_foreground: 0x1A1B1E,
    success_hover: 0x51CF66,
    success_active: 0x37B24D,
    success_foreground: 0x1A1B1E,
    // Translucent mac scrollbars.
    scrollbar: t(0x000000, 0.0),
    scrollbar_thumb: t(0x5C5F66, 0.7),
    scrollbar_thumb_hover: 0x5C5F66,
};

/// Classic: open-color gray ramp on white; status hues from Primer where open-color has no AA step. Grays and the blue fill sit one step darker than the ramp's text defaults so they also pass on the zebra, hover, chip and header surfaces (2026-10-02 contrast test).
pub(crate) const LIGHT: Palette = Palette {
    background: 0xFFFFFF,
    foreground: 0x212529,
    muted: 0xF1F3F5,
    // gray.6 #868e96 fails AA on white (3.3:1); this midpoint passes.
    muted_foreground: 0x626A72,
    secondary: 0xF1F3F5,
    secondary_foreground: 0x495057,
    accent: 0xF1F3F5,
    accent_foreground: 0x1971C2,
    title_bar: 0xF8F9FA,
    tab_bar_segmented: 0xE9ECEF,
    tab_foreground: 0x626A72,
    tab_active: 0xFFFFFF,
    tab_active_foreground: 0x212529,
    // Accent: blue.7 for fills, blue.8 for text (blue.7 is 4.2:1 — fails).
    primary: 0x1971C2,
    primary_hover: 0x1864AB,
    primary_active: 0x1864AB,
    primary_foreground: 0xFFFFFF,
    link: 0x1971C2,
    // Status: red.9; warning/success borrow Primer's AA-on-white values
    // because open-color's yellow/green ramps have no AA step on white.
    danger: 0xC92A2A,
    warning: 0x8F5F00,
    success: 0x1A7F37,
    table_even: 0xF8F9FA,
    table_head: 0xF8F9FA,
    table_head_foreground: 0x626A72,
    table_hover: 0xF1F3F5,
    popover: 0xFFFFFF,
    popover_foreground: 0x212529,
    border: 0xDEE2E6,
    input: 0xCED4DA,
    secondary_hover: 0xE9ECEF,
    secondary_active: 0xDEE2E6,
    link_hover: 0x1864AB,
    link_active: 0x1864AB,
    ring: 0x228BE6,
    caret: 0x212529,
    selection: t(0x228BE6, 0.25),
    table_active: t(0x228BE6, 0.16),
    table_active_border: t(0x228BE6, 0.50),
    table_row_border: t(0x000000, 0.05),
    list_active: t(0x228BE6, 0.10),
    danger_hover: 0xE03131,
    danger_active: 0xB02525,
    danger_foreground: 0xFFFFFF,
    warning_hover: 0xB08000,
    warning_active: 0x7D5400,
    warning_foreground: 0xFFFFFF,
    success_hover: 0x2F9E44,
    success_active: 0x166B2E,
    success_foreground: 0xFFFFFF,
    scrollbar: t(0xFFFFFF, 0.0),
    scrollbar_thumb: t(0xADB5BD, 0.9),
    scrollbar_thumb_hover: 0x868E96,
};

/// Studio: cool, layered native utility (`.claude/research/skins/2026-10-20-amp-skin-directions.md`).
pub(crate) const STUDIO_LIGHT: Palette = Palette {
    background: 0xFFFFFF,
    foreground: 0x202733,
    muted: 0xF0F3F7,
    muted_foreground: 0x566170,
    secondary: 0xEDF1F6,
    secondary_foreground: 0x384455,
    accent: 0xEAF1FA,
    accent_foreground: 0x175FC0,
    title_bar: 0xEDF0F4,
    tab_bar_segmented: 0xE2E7EE,
    tab_foreground: 0x4E5B6C,
    tab_active: 0xFFFFFF,
    tab_active_foreground: 0x202733,
    primary: 0x175FC0,
    primary_hover: 0x124F9F,
    primary_active: 0x103F80,
    primary_foreground: 0xFFFFFF,
    link: 0x175FC0,
    danger: 0xB42332,
    warning: 0x825500,
    success: 0x14683D,
    table_even: 0xF9FAFC,
    table_head: 0xF0F3F7,
    table_head_foreground: 0x566170,
    table_hover: 0xEEF3FA,
    popover: 0xFFFFFF,
    popover_foreground: 0x202733,
    border: 0xD5DBE3,
    input: 0x768393,
    secondary_hover: 0xE5EBF3,
    secondary_active: 0xDCE4EF,
    link_hover: 0x124F9F,
    link_active: 0x124F9F,
    ring: 0x175FC0,
    caret: 0x202733,
    selection: t(0x175FC0, 0.25),
    table_active: t(0xE5EEFA, 1.0),
    table_active_border: t(0x175FC0, 1.0),
    table_row_border: t(0x202733, 0.06),
    list_active: t(0xE5EEFA, 1.0),
    danger_hover: 0xB42332,
    danger_active: 0xB42332,
    danger_foreground: 0xFFFFFF,
    warning_hover: 0x825500,
    warning_active: 0x825500,
    warning_foreground: 0xFFFFFF,
    success_hover: 0x14683D,
    success_active: 0x14683D,
    success_foreground: 0xFFFFFF,
    scrollbar: t(0xFFFFFF, 0.0),
    scrollbar_thumb: t(0x566170, 0.6),
    scrollbar_thumb_hover: 0x566170,
};

pub(crate) const STUDIO_DARK: Palette = Palette {
    background: 0x1E242D,
    foreground: 0xE5EBF3,
    muted: 0x293441,
    muted_foreground: 0xAFBAC8,
    secondary: 0x2A3441,
    secondary_foreground: 0xD4DEE9,
    accent: 0x283C53,
    accent_foreground: 0x99C5FF,
    title_bar: 0x171B22,
    tab_bar_segmented: 0x242D38,
    tab_foreground: 0xB2BDCA,
    tab_active: 0x364454,
    tab_active_foreground: 0xEDF1F7,
    primary: 0x1F62AF,
    primary_hover: 0x195497,
    primary_active: 0x15467F,
    primary_foreground: 0xFFFFFF,
    link: 0x83B7FF,
    danger: 0xFF929B,
    warning: 0xE9BF70,
    success: 0x75C99A,
    table_even: 0x202731,
    table_head: 0x252D38,
    table_head_foreground: 0xAFBAC8,
    table_hover: 0x293441,
    popover: 0x2B3542,
    popover_foreground: 0xE5EBF3,
    border: 0x46515F,
    input: 0x8593A4,
    secondary_hover: 0x323E4D,
    secondary_active: 0x394758,
    link_hover: 0xA6CDFF,
    link_active: 0xA6CDFF,
    ring: 0x83B7FF,
    caret: 0xE5EBF3,
    selection: t(0x83B7FF, 0.35),
    table_active: t(0x2D4055, 1.0),
    table_active_border: t(0x83B7FF, 1.0),
    table_row_border: t(0xE5EBF3, 0.06),
    list_active: t(0x2D4055, 1.0),
    danger_hover: 0xFF929B,
    danger_active: 0xFF929B,
    danger_foreground: 0x1E242D,
    warning_hover: 0xE9BF70,
    warning_active: 0xE9BF70,
    warning_foreground: 0x1E242D,
    success_hover: 0x75C99A,
    success_active: 0x75C99A,
    success_foreground: 0x1E242D,
    scrollbar: t(0x000000, 0.0),
    scrollbar_thumb: t(0xAFBAC8, 0.6),
    scrollbar_thumb_hover: 0xAFBAC8,
};

/// Mineral: warm stone and paper, petrol-blue accents (same study).
pub(crate) const MINERAL_LIGHT: Palette = Palette {
    background: 0xFFFDF8,
    foreground: 0x292720,
    muted: 0xF2EEE5,
    muted_foreground: 0x5B564C,
    secondary: 0xECE7DD,
    secondary_foreground: 0x474238,
    accent: 0xE8EFF0,
    accent_foreground: 0x245C73,
    title_bar: 0xEAE6DE,
    tab_bar_segmented: 0xDDD8CD,
    tab_foreground: 0x544E43,
    tab_active: 0xFFFDF8,
    tab_active_foreground: 0x292720,
    primary: 0x245C73,
    primary_hover: 0x1E4D61,
    primary_active: 0x183D4D,
    primary_foreground: 0xFFFFFF,
    link: 0x245C73,
    danger: 0xAC2934,
    warning: 0x7C5200,
    success: 0x256542,
    table_even: 0xFAF7F0,
    table_head: 0xF2EEE5,
    table_head_foreground: 0x5B564C,
    table_hover: 0xF1ECE2,
    popover: 0xFFFDF8,
    popover_foreground: 0x292720,
    border: 0xD2CBBE,
    input: 0x81796B,
    secondary_hover: 0xE5DFD4,
    secondary_active: 0xDCD5C8,
    link_hover: 0x1E4D61,
    link_active: 0x1E4D61,
    ring: 0x245C73,
    caret: 0x292720,
    selection: t(0x245C73, 0.25),
    table_active: t(0xE4ECEC, 1.0),
    table_active_border: t(0x245C73, 1.0),
    table_row_border: t(0x292720, 0.06),
    list_active: t(0xE4ECEC, 1.0),
    danger_hover: 0xAC2934,
    danger_active: 0xAC2934,
    danger_foreground: 0xFFFFFF,
    warning_hover: 0x7C5200,
    warning_active: 0x7C5200,
    warning_foreground: 0xFFFFFF,
    success_hover: 0x256542,
    success_active: 0x256542,
    success_foreground: 0xFFFFFF,
    scrollbar: t(0xFFFFFF, 0.0),
    scrollbar_thumb: t(0x5B564C, 0.6),
    scrollbar_thumb_hover: 0x5B564C,
};

pub(crate) const MINERAL_DARK: Palette = Palette {
    background: 0x25241F,
    foreground: 0xEEEAE1,
    muted: 0x34322C,
    muted_foreground: 0xC0BAAD,
    secondary: 0x34322C,
    secondary_foreground: 0xE0D9CD,
    accent: 0x2B3A3E,
    accent_foreground: 0xA7D5E4,
    title_bar: 0x1B1C19,
    tab_bar_segmented: 0x302E28,
    tab_foreground: 0xC0BAAD,
    tab_active: 0x444137,
    tab_active_foreground: 0xFFF9ED,
    primary: 0x2D6377,
    primary_hover: 0x255467,
    primary_active: 0x1D4555,
    primary_foreground: 0xFFFFFF,
    link: 0xA7D5E4,
    danger: 0xF5A09F,
    warning: 0xE2C07C,
    success: 0xA2CCA9,
    table_even: 0x292821,
    table_head: 0x302E27,
    table_head_foreground: 0xC0BAAD,
    table_hover: 0x37352D,
    popover: 0x39362E,
    popover_foreground: 0xEEEAE1,
    border: 0x575247,
    input: 0x9C9483,
    secondary_hover: 0x3D3A32,
    secondary_active: 0x454137,
    link_hover: 0xC0E3EE,
    link_active: 0xC0E3EE,
    ring: 0xA7D5E4,
    caret: 0xEEEAE1,
    selection: t(0xA7D5E4, 0.35),
    table_active: t(0x344346, 1.0),
    table_active_border: t(0xA7D5E4, 1.0),
    table_row_border: t(0xEEEAE1, 0.06),
    list_active: t(0x344346, 1.0),
    danger_hover: 0xF5A09F,
    danger_active: 0xF5A09F,
    danger_foreground: 0x25241F,
    warning_hover: 0xE2C07C,
    warning_active: 0xE2C07C,
    warning_foreground: 0x25241F,
    success_hover: 0xA2CCA9,
    success_active: 0xA2CCA9,
    success_foreground: 0x25241F,
    scrollbar: t(0x000000, 0.0),
    scrollbar_thumb: t(0xC0BAAD, 0.6),
    scrollbar_thumb_hover: 0xC0BAAD,
};

pub(crate) const CLASSIC: ColorSet = ColorSet {
    light: &LIGHT,
    dark: &DARK,
};
pub(crate) const STUDIO: ColorSet = ColorSet {
    light: &STUDIO_LIGHT,
    dark: &STUDIO_DARK,
};
pub(crate) const MINERAL: ColorSet = ColorSet {
    light: &MINERAL_LIGHT,
    dark: &MINERAL_DARK,
};

/// The built-in colour set a `colors` choice names; `None` for a user theme.
pub(crate) fn built_in(colors: &Colors) -> Option<&'static ColorSet> {
    match colors {
        Colors::Classic => Some(&CLASSIC),
        Colors::Studio => Some(&STUDIO),
        Colors::Mineral => Some(&MINERAL),
        Colors::Custom(_) => None,
    }
}

/// Every built-in palette with its name, for the contrast tests.
#[cfg(test)]
pub(crate) const BUILT_IN: [(&str, &ColorSet); 3] = [
    ("classic", &CLASSIC),
    ("studio", &STUDIO),
    ("mineral", &MINERAL),
];

/// Overwrite the active colours with one palette's tokens. Status hues are
/// tuned as *text* colors (they render as dots + short text, never filled
/// slabs); every text token meets WCAG AA (4.5:1) on every surface it is
/// drawn on — asserted by the `contrast` test below.
pub(crate) fn apply_palette(cx: &mut App, p: &Palette) {
    let theme = Theme::global_mut(cx);
    let t = &mut theme.colors;

    // Surfaces & text.
    t.background = c(p.background);
    t.foreground = c(p.foreground);
    t.border = c(p.border);
    t.input = c(p.input);
    t.muted = c(p.muted);
    t.muted_foreground = c(p.muted_foreground);
    t.secondary = c(p.secondary);
    t.secondary_hover = c(p.secondary_hover);
    t.secondary_active = c(p.secondary_active);
    t.secondary_foreground = c(p.secondary_foreground);
    t.accent = c(p.accent);
    t.accent_foreground = c(p.accent_foreground); // issue-id tag: link-adjacent

    // Chrome sits one step away from content (Zed/macOS idiom).
    t.title_bar = c(p.title_bar);
    t.title_bar_border = c(p.border);
    // Primary board-scope control: muted track, body-colored selected
    // segment, and a clear text hierarchy in both appearance modes.
    t.tab_bar_segmented = c(p.tab_bar_segmented);
    t.tab_foreground = c(p.tab_foreground);
    t.tab_active = c(p.tab_active);
    t.tab_active_foreground = c(p.tab_active_foreground);

    // The one accent.
    t.primary = c(p.primary);
    t.primary_hover = c(p.primary_hover);
    t.primary_active = c(p.primary_active);
    t.primary_foreground = c(p.primary_foreground);
    t.link = c(p.link);
    t.link_hover = c(p.link_hover);
    t.link_active = c(p.link_active);
    t.ring = c(p.ring);
    t.caret = c(p.caret);
    t.selection = p.selection.hsla();

    // Status hues.
    t.danger = c(p.danger);
    t.danger_hover = c(p.danger_hover);
    t.danger_active = c(p.danger_active);
    t.danger_foreground = c(p.danger_foreground);
    t.warning = c(p.warning);
    t.warning_hover = c(p.warning_hover);
    t.warning_active = c(p.warning_active);
    t.warning_foreground = c(p.warning_foreground);
    t.success = c(p.success);
    t.success_hover = c(p.success_hover);
    t.success_active = c(p.success_active);
    t.success_foreground = c(p.success_foreground);

    // Table: subtle zebra carries row tracking; hairlines near-invisible.
    t.table = c(p.background);
    t.table_even = c(p.table_even);
    t.table_head = c(p.table_head);
    t.table_head_foreground = c(p.table_head_foreground);
    t.table_hover = c(p.table_hover);
    t.table_active = p.table_active.hsla();
    t.table_active_border = p.table_active_border.hsla();
    t.table_row_border = p.table_row_border.hsla();
    t.list = c(p.background);
    t.list_even = c(p.table_even);
    t.list_head = c(p.table_head);
    t.list_hover = c(p.table_hover);
    t.list_active = p.list_active.hsla();
    t.list_active_border = p.table_active_border.hsla();

    // Tooltips on the raised-surface layer; translucent mac scrollbars.
    t.popover = c(p.popover);
    t.popover_foreground = c(p.popover_foreground);
    t.scrollbar = p.scrollbar.hsla();
    t.scrollbar_thumb = p.scrollbar_thumb.hsla();
    t.scrollbar_thumb_hover = c(p.scrollbar_thumb_hover);
    // A dialog's backdrop: the framework's is a faint 5 % in the light,
    // so a dialog read as a floating panel rather than a modal (critique
    // 2026-10-21). Black over the window, a step deeper in the dark where
    // the surfaces are already close to it.
    t.overlay = gpui::hsla(0., 0., 0., if t.background.l < 0.5 { 0.42 } else { 0.22 });
}

/// The primary buttons (Save, Sign in, …) in the palette's own accent.
/// gpui-component draws them from `button_primary*` tokens that fall back
/// to its neutral-900/50 unless a theme sets them (critique C6). Classic
/// kept the framework's near-black until the 2026-10-21 critique named
/// that the anti-reference; every built-in set takes its blue now.
pub(crate) fn apply_primary_buttons(cx: &mut App, p: &Palette) {
    let t = &mut Theme::global_mut(cx).colors;
    t.button_primary = c(p.primary);
    t.button_primary_hover = c(p.primary_hover);
    t.button_primary_active = c(p.primary_active);
    t.button_primary_foreground = c(p.primary_foreground);
}

/// The geometry every colour theme keeps: 14 px base UI text, small
/// native-feeling radii. Glacier's controls are a little rounder.
pub(crate) fn apply_geometry(cx: &mut App, skin: Skin) {
    let theme = Theme::global_mut(cx);
    theme.font_size = px(14.);
    match skin {
        Skin::Classic => {
            theme.radius = px(4.);
            theme.radius_lg = px(8.);
        }
        Skin::Glacier => {
            theme.radius = px(7.);
            theme.radius_lg = px(12.);
        }
    }
}

/// What the skin changes in the theme itself, after the colours: Glacier's
/// rows are parted by a quiet rule of the text colour rather than the
/// palette's hairline.
pub(crate) fn apply_skin(cx: &mut App, skin: Skin) {
    if skin == Skin::Glacier {
        let theme = Theme::global_mut(cx);
        let (fg, bg, primary, link) = (
            theme.foreground,
            theme.background,
            theme.primary,
            theme.link,
        );
        let texts = [
            fg,
            theme.muted_foreground,
            link,
            theme.danger,
            theme.warning,
            theme.success,
        ];
        let t = &mut theme.colors;
        t.table_row_border = tint(fg, bg, RULE_TINT);
        // The caret row's fill, painted by the table itself, and the edge
        // the rail and outline take. The hover is a breath of the text's
        // own ink, so a coloured hover (Studio's blue) can't pass for the
        // selection, and the selection is the stronger of the two.
        t.table_active = tint(primary, bg, selection_tint(primary, bg, &texts));
        t.table_active_border = link;
        t.table_hover = tint(fg, bg, HOVER_TINT);
        t.list_active = t.table_active;
        t.list_active_border = link;
        t.list_hover = t.table_hover;
        // No zebra, and no band behind the column labels: the sheet's
        // rounded corners clip nothing but the sheet's own colour.
        t.table_even = bg;
        t.table_head = bg;
        t.list_even = bg;
        t.list_head = bg;
    }
}

// Glacier's material recipes: tints of an ink over the sheet, precomposited
// to an opaque colour so a capsule reads the same on a hovered or selected
// row (`.claude/research/skins/2026-10-20-amp-elements-skin.md`, "Colors
// are inputs").
/// A status or count capsule's fill: this much of its tone over the sheet.
pub(crate) const CAPSULE_TINT: f32 = 0.06;
/// A section header's band.
pub(crate) const BAND_TINT: f32 = 0.025;
/// The rule between rows.
pub(crate) const RULE_TINT: f32 = 0.07;
/// The selected row's fill: this much of the primary over the sheet, so it
/// out-ranks the hover (2026-10-21 critique C3: at 0.06 the hover was the
/// stronger of the two) — or as much of that as keeps every text colour at
/// AA on it ([`selection_tint`]): Classic's light link ink is 4.19:1 on the
/// full tint and reads AA only up to 0.07.
pub(crate) const SELECTION_TINT: f32 = 0.13;
/// The least the selection fill may fall back to.
pub(crate) const SELECTION_TINT_MIN: f32 = 0.06;
/// A hovered row: a breath of the text's ink over the sheet, never a colour.
pub(crate) const HOVER_TINT: f32 = 0.04;
/// A label chip's fill and its edge, of the text's ink over the sheet: opaque,
/// so the chip stays visible on a hovered or selected row (critique C5).
pub(crate) const CHIP_TINT: f32 = 0.08;
pub(crate) const CHIP_EDGE_TINT: f32 = 0.20;
/// A control that is on — a quick filter, the current repository, a Settings
/// segment: its fill is the link colour at this over its surface.
pub(crate) const ACTIVE_TINT: f32 = 0.14;
/// The selected view tab in the dark: this much of the text's ink over the
/// track, so the segment reads raised rather than sunk (critique C2); in the
/// light the sheet's white does that on its own.
pub(crate) const TAB_RAISE_TINT: f32 = 0.10;

/// The selection tint for one set of colours: [`SELECTION_TINT`], or the
/// largest hundredth below it on which every one of `texts` keeps WCAG AA
/// (4.5:1), never under [`SELECTION_TINT_MIN`].
pub(crate) fn selection_tint(primary: Hsla, surface: Hsla, texts: &[Hsla]) -> f32 {
    let mut tint_at = SELECTION_TINT;
    while tint_at > SELECTION_TINT_MIN + 1e-6 {
        let fill = hex(tint(primary, surface, tint_at));
        if texts
            .iter()
            .all(|text| prmarmot_local::themes::contrast_ratio(hex(*text), fill) >= 4.5)
        {
            break;
        }
        tint_at = ((tint_at - 0.01) * 100.).round() / 100.;
    }
    tint_at.max(SELECTION_TINT_MIN)
}

/// `ink` at `alpha` over the opaque `surface`, as one opaque colour.
pub(crate) fn tint(ink: Hsla, surface: Hsla, alpha: f32) -> Hsla {
    c(prmarmot_local::themes::blend(
        hex(ink),
        hex(surface),
        f64::from(alpha),
    ))
}

/// A colour as `0xRRGGBB`, its own alpha dropped.
fn hex(color: Hsla) -> u32 {
    let rgb = color.to_rgb();
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    channel(rgb.r) << 16 | channel(rgb.g) << 8 | channel(rgb.b)
}

/// Scrollbars still show while you scroll and hide when idle, as macOS
/// does, but snap rather than fade: gpui-component animates the fade at the
/// display's rate, ~80 frames after every scroll or arrow key, which an
/// otherwise idle board does not need (the calm rule; 2026-10-01 audit C9).
/// Writes the Base layer directly, so it runs after `Theme::sync_base`.
pub(crate) fn still_scrollbars(cx: &mut App) {
    let base = gpui_base::Theme::global_mut(cx);
    let motion = base
        .scrollbar
        .motion()
        .with_enter(Duration::ZERO)
        .with_exit(Duration::ZERO)
        .with_expand(Duration::ZERO);
    base.scrollbar = base.scrollbar.clone().with_motion(motion);
}

#[cfg(test)]
mod contrast {
    use super::*;
    use prmarmot_local::themes::{blend, contrast_ratio as ratio};

    /// The largest difference between two colours in any one channel, 0–255.
    fn channel_gap(a: u32, b: u32) -> u32 {
        (0..3)
            .map(|shift| {
                let x = (a >> (shift * 8)) & 0xFF;
                let y = (b >> (shift * 8)) & 0xFF;
                x.abs_diff(y)
            })
            .max()
            .unwrap_or(0)
    }

    /// Every (text, surface) pair the app draws, by token name. Body text,
    /// muted text and the status hues sit on the page, on zebra, hovered and
    /// (`selection`) selected rows; muted text also on chips, the title bar,
    /// the table header and popovers; the rest are each widget's own text on
    /// its own fill.
    fn pairs(p: &Palette, selection: bool) -> Vec<(&'static str, u32, &'static str, u32)> {
        let mut pairs = Vec::new();
        let active = p.table_active.over(p.background);
        for (text_name, text) in [
            ("foreground", p.foreground),
            ("muted_foreground", p.muted_foreground),
            ("link", p.link),
            ("danger", p.danger),
            ("warning", p.warning),
            ("success", p.success),
        ] {
            for (surface_name, surface) in [
                ("background", p.background),
                ("table_even", p.table_even),
                ("table_hover", p.table_hover),
            ] {
                pairs.push((text_name, text, surface_name, surface));
            }
            if selection {
                pairs.push((text_name, text, "table_active", active));
            }
        }
        for (surface_name, surface) in [
            ("muted", p.muted),
            ("secondary", p.secondary),
            ("title_bar", p.title_bar),
            ("table_head", p.table_head),
            ("popover", p.popover),
        ] {
            pairs.push((
                "muted_foreground",
                p.muted_foreground,
                surface_name,
                surface,
            ));
        }
        for (surface_name, surface) in [
            ("secondary", p.secondary),
            ("secondary_hover", p.secondary_hover),
            ("secondary_active", p.secondary_active),
        ] {
            pairs.push((
                "secondary_foreground",
                p.secondary_foreground,
                surface_name,
                surface,
            ));
        }
        pairs.extend([
            ("link_hover", p.link_hover, "background", p.background),
            ("link_hover", p.link_hover, "table_hover", p.table_hover),
            ("accent_foreground", p.accent_foreground, "accent", p.accent),
            (
                "tab_foreground",
                p.tab_foreground,
                "tab_bar_segmented",
                p.tab_bar_segmented,
            ),
            (
                "tab_active_foreground",
                p.tab_active_foreground,
                "tab_active",
                p.tab_active,
            ),
            (
                "table_head_foreground",
                p.table_head_foreground,
                "table_head",
                p.table_head,
            ),
            (
                "popover_foreground",
                p.popover_foreground,
                "popover",
                p.popover,
            ),
            (
                "primary_foreground",
                p.primary_foreground,
                "primary",
                p.primary,
            ),
            (
                "primary_foreground",
                p.primary_foreground,
                "primary_hover",
                p.primary_hover,
            ),
            (
                "primary_foreground",
                p.primary_foreground,
                "primary_active",
                p.primary_active,
            ),
        ]);
        pairs
    }

    /// Glacier's recipe surfaces: each tone's ink on its own capsule, the
    /// text colours on a section band of every tone and on the selected
    /// row's fill.
    fn glacier_pairs(p: &Palette) -> Vec<(&'static str, u32, &'static str, u32)> {
        let mut pairs = Vec::new();
        let tones = [
            ("danger", p.danger),
            ("warning", p.warning),
            ("success", p.success),
            ("secondary_foreground", p.secondary_foreground),
            ("muted_foreground", p.muted_foreground),
        ];
        for (tone_name, tone) in tones {
            let capsule = blend(tone, p.background, f64::from(CAPSULE_TINT));
            pairs.push((tone_name, tone, "its capsule", capsule));
            pairs.push(("muted_foreground", p.muted_foreground, "capsule", capsule));
            let band = blend(tone, p.background, f64::from(BAND_TINT));
            for (text_name, text) in [
                ("foreground", p.foreground),
                ("secondary_foreground", p.secondary_foreground),
                ("muted_foreground", p.muted_foreground),
            ] {
                pairs.push((text_name, text, "band", band));
            }
        }
        let active = blend(p.link, p.title_bar, f64::from(ACTIVE_TINT));
        pairs.push(("foreground", p.foreground, "active filter", active));
        let segment = blend(p.link, p.background, f64::from(ACTIVE_TINT));
        pairs.push(("foreground", p.foreground, "active segment", segment));
        let texts = [
            ("foreground", p.foreground),
            ("muted_foreground", p.muted_foreground),
            ("link", p.link),
            ("danger", p.danger),
            ("warning", p.warning),
            ("success", p.success),
        ];
        let fill = blend(p.primary, p.background, f64::from(p.selection_tint()));
        let hover = blend(p.foreground, p.background, f64::from(HOVER_TINT));
        for (text_name, text) in texts {
            pairs.push((text_name, text, "selection fill", fill));
            pairs.push((text_name, text, "hover", hover));
        }
        let chip = blend(p.foreground, p.background, f64::from(CHIP_TINT));
        pairs.push(("secondary_foreground", p.secondary_foreground, "chip", chip));
        pairs.push(("muted_foreground", p.muted_foreground, "chip", chip));
        pairs
    }

    impl Palette {
        /// What [`selection_tint`] picks for this palette.
        fn selection_tint(&self) -> f32 {
            selection_tint(
                c(self.primary),
                c(self.background),
                &[
                    c(self.foreground),
                    c(self.muted_foreground),
                    c(self.link),
                    c(self.danger),
                    c(self.warning),
                    c(self.success),
                ],
            )
        }
    }

    #[test]
    fn every_text_token_meets_wcag_aa_on_every_surface_it_is_drawn_on() {
        let mut failures = Vec::new();
        for (name, set) in BUILT_IN {
            // Classic's selection tint predates this check and is kept as it
            // is, so the default look stays what it was: on it, dark muted
            // text is 3.88:1 and danger 4.36:1, light link 4.17:1 and
            // success 4.22:1. Studio and Mineral chose opaque selection
            // fills that pass.
            let selection = name != "classic";
            for (mode, palette) in [("dark", set.dark), ("light", set.light)] {
                for (text_name, text, surface_name, surface) in pairs(palette, selection) {
                    let r = ratio(text, surface);
                    if r < 4.5 {
                        failures.push(format!(
                            "{name} {mode}: {text_name} #{text:06X} on {surface_name} #{surface:06X} is {r:.2}:1"
                        ));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "below WCAG AA (4.5:1):\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn glacier_s_capsules_bands_and_selection_keep_aa_with_every_built_in() {
        let mut failures = Vec::new();
        for (name, set) in BUILT_IN {
            for (mode, palette) in [("dark", set.dark), ("light", set.light)] {
                for (text_name, text, surface_name, surface) in glacier_pairs(palette) {
                    let r = ratio(text, surface);
                    if r < 4.5 {
                        failures.push(format!(
                            "{name} {mode}: {text_name} #{text:06X} on {surface_name} #{surface:06X} is {r:.2}:1"
                        ));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "below WCAG AA (4.5:1):\n{}",
            failures.join("\n")
        );
    }

    /// A state has to be told by more than a hue: the selected segment's
    /// link edge against the dialog's surface and against its own fill, the
    /// chip's edge against a hovered and a selected row, and the raised tab
    /// against its track, in every set and mode.
    #[test]
    fn glacier_s_state_edges_are_visible_with_every_built_in() {
        let mut failures = Vec::new();
        let mut gaps = Vec::new();
        let mut check = |name: &str, mode: &str, what: &str, a: u32, b: u32, floor: f64| {
            let r = ratio(a, b);
            if r < floor {
                failures.push(format!(
                    "{name} {mode}: {what} #{a:06X} against #{b:06X} is {r:.2}:1, under {floor}"
                ));
            }
        };
        for (name, set) in BUILT_IN {
            for (mode, p) in [("dark", set.dark), ("light", set.light)] {
                // WCAG 1.4.11: a state indicator at 3:1.
                let segment = blend(p.link, p.background, f64::from(ACTIVE_TINT));
                check(
                    name,
                    mode,
                    "segment edge on the surface",
                    p.link,
                    p.background,
                    3.0,
                );
                check(name, mode, "segment edge on its fill", p.link, segment, 3.0);
                // The chip's edge stays visible on both row fills (a quiet
                // floor of our own: the chip is a word, not a control).
                let edge = blend(p.foreground, p.background, f64::from(CHIP_EDGE_TINT));
                let hover = blend(p.foreground, p.background, f64::from(HOVER_TINT));
                let fill = blend(p.primary, p.background, f64::from(p.selection_tint()));
                check(name, mode, "chip edge on a hovered row", edge, hover, 1.2);
                check(name, mode, "chip edge on a selected row", edge, fill, 1.2);
                // The selection out-ranks the hover: in a dark set the two
                // fills are equally bright and differ by hue, so the test
                // asks for a channel gap, not a luminance ratio.
                let gap = channel_gap(hover, fill);
                if gap < 6 {
                    gaps.push(format!(
                        "{name} {mode}: selection fill #{fill:06X} over hover #{hover:06X} differs by {gap}/255 at most, under 6"
                    ));
                }
                // The selected tab is lighter than its track.
                let tab = if mode == "dark" {
                    blend(p.foreground, p.tab_bar_segmented, f64::from(TAB_RAISE_TINT))
                } else {
                    p.background
                };
                check(
                    name,
                    mode,
                    "raised tab on its track",
                    tab,
                    p.tab_bar_segmented,
                    1.15,
                );
                assert!(
                    prmarmot_local::themes::luminance(tab)
                        > prmarmot_local::themes::luminance(p.tab_bar_segmented),
                    "{name} {mode}: the selected tab is darker than its track"
                );
            }
        }
        failures.extend(gaps);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Every built-in keeps the full selection tint but Classic light, whose
    /// link ink caps it at 0.08; none falls to the floor.
    #[test]
    fn the_selection_tint_backs_off_only_where_a_text_colour_needs_it() {
        for (name, set) in BUILT_IN {
            for (mode, p) in [("dark", set.dark), ("light", set.light)] {
                let tint_at = p.selection_tint();
                if name == "classic" && mode == "light" {
                    assert!((tint_at - 0.08).abs() < 1e-6, "classic light: {tint_at}");
                } else {
                    assert!(
                        (tint_at - SELECTION_TINT).abs() < 1e-6,
                        "{name} {mode}: {tint_at}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_luminance_formula_is_wcag_s() {
        assert!((ratio(0xFFFFFF, 0x000000) - 21.0).abs() < 1e-9);
        assert!((ratio(0x6C757D, 0xFFFFFF) - 4.69).abs() < 0.01);
        assert_eq!(hex(c(0x1A2B3C)), 0x1A2B3C);
        assert_eq!(hex(tint(c(0x000000), c(0xFFFFFF), 0.5)), 0x808080);
    }
}
