//! The look: which colours the board draws with and how its elements are
//! drawn. Three independent settings — Appearance (`theme`, light / dark /
//! system, in `theme.rs`), Colours (`colors`: Classic, Studio, Mineral or a
//! theme file from the themes folder) and Skin (`skin`: Classic, today's
//! full-bleed table, or Glacier, the table on an inset sheet). Spec:
//! `.claude/research/skins/2026-10-20-amp-elements-skin.md`.
//!
//! The defaults draw exactly what the app drew before the look existed.

use std::{rc::Rc, sync::Arc};

use gpui::{
    canvas, point, px, size, App, Bounds, BoxShadow, Canvas, ContentMask, Global, Hsla, Image,
    ImageFormat, RenderImage, Styled, Window, WindowBackgroundAppearance,
};
use gpui_component::theme::{ThemeConfig, ThemeSet};
use gpui_component::ActiveTheme;
use prmarmot_local::config::{Colors, FileConfig, Skin};
use prmarmot_local::themes::{ThemeListing, UserTheme};

/// The look the window draws with. A gpui global, read by every view.
#[derive(Clone, Default)]
pub struct Look {
    pub colors: Colors,
    pub skin: Skin,
    /// The theme files in the themes folder, read when the look was loaded.
    pub listing: ThemeListing,
    /// The parsed theme configs, one per listed theme, in listing order.
    configs: Vec<Rc<ThemeConfig>>,
    /// Why the chosen colours could not be used, if they could not.
    pub warning: Option<String>,
}

impl Global for Look {}

impl Look {
    /// The look the file asks for, with every theme file in the themes
    /// folder read; a `colors` naming a theme that is not there falls back
    /// to Classic with a warning.
    pub fn from_file(file: &FileConfig) -> Self {
        Self::load(
            prmarmot_local::config::colors(file),
            prmarmot_local::config::skin(file),
        )
    }

    /// [`Self::from_file`] for a choice already read.
    pub fn load(colors: Colors, skin: Skin) -> Self {
        let listing = prmarmot_local::themes::list_themes();
        let mut look = Look {
            colors: Colors::Classic,
            skin,
            listing,
            configs: Vec::new(),
            warning: None,
        };
        look.load_configs();
        match &colors {
            Colors::Custom(name) => match look.listing.find(name) {
                // The listing matched the name regardless of case; keep the
                // theme's own spelling so Settings finds it too.
                Some(theme) => look.colors = Colors::Custom(theme.name.clone()),
                None => {
                    look.warning = Some(format!(
                        "the theme \"{name}\" is not in {}; drawing with Classic colours",
                        prmarmot_local::themes::themes_dir().display()
                    ));
                }
            },
            _ => look.colors = colors,
        }
        look
    }

    /// Parse every listed theme file into gpui-component's own theme
    /// config; a file that lists but does not parse (the listing checks the
    /// shape, this the colours) is dropped from the listing with a problem.
    fn load_configs(&mut self) {
        let mut kept = Vec::new();
        let mut configs = Vec::new();
        for theme in std::mem::take(&mut self.listing.themes) {
            match parse_theme(&theme) {
                Ok(config) => {
                    kept.push(theme);
                    configs.push(config);
                }
                Err(problem) => self.listing.problems.push(problem),
            }
        }
        self.listing.themes = kept;
        self.configs = configs;
    }

    /// The user theme for one mode: the chosen theme's sibling of that
    /// mode from the same file when it has one, else the theme itself.
    pub fn user_theme(&self, dark: bool) -> Option<(&UserTheme, &Rc<ThemeConfig>)> {
        let Colors::Custom(name) = &self.colors else {
            return None;
        };
        let theme = self.listing.for_mode(name, dark)?;
        let ix = self
            .listing
            .themes
            .iter()
            .position(|t| std::ptr::eq(t, theme))?;
        Some((theme, self.configs.get(ix)?))
    }
}

/// The named theme inside a theme-set file, as a config to apply.
fn parse_theme(theme: &UserTheme) -> Result<Rc<ThemeConfig>, String> {
    let text = std::fs::read_to_string(&theme.path)
        .map_err(|e| format!("ignoring theme file {}: {e}", theme.path.display()))?;
    let set: ThemeSet = serde_json::from_str(&text)
        .map_err(|e| format!("ignoring theme file {}: {e}", theme.path.display()))?;
    set.themes
        .into_iter()
        .find(|config| config.name.as_ref() == theme.name && config.mode.is_dark() == theme.dark)
        .map(Rc::new)
        .ok_or_else(|| {
            format!(
                "ignoring theme file {}: no theme named {:?}",
                theme.path.display(),
                theme.name
            )
        })
}

/// The look; `main` sets it before the first window opens.
pub fn look(cx: &App) -> &Look {
    cx.global::<Look>()
}

/// The skin in force, Classic before a look is set.
pub fn skin(cx: &App) -> Skin {
    cx.try_global::<Look>()
        .map_or(Skin::Classic, |look| look.skin)
}

/// Whether the elements are drawn Glacier's way.
pub fn glacier(cx: &App) -> bool {
    skin(cx) == Skin::Glacier
}

/// Window material is independent of the editable look: cancelling a preview
/// must not restore stale system accessibility preferences or decode textures.
pub struct Chrome {
    translucent: bool,
    grain: Option<Arc<RenderImage>>,
}

impl Global for Chrome {}

impl Chrome {
    pub fn sync(window: &mut Window, cx: &mut App) {
        let translucent = cfg!(target_os = "macos") && !crate::platform::reduce_transparency();
        if !cx.has_global::<Self>() {
            cx.set_global(Self {
                translucent,
                grain: None,
            });
        }
        cx.global_mut::<Self>().translucent = translucent;
        // One 128² RGBA tile (64 KiB before the renderer's atlas allocation),
        // decoded once and reused across modes. No viewport-sized texture.
        if glacier(cx) && translucent && cx.global::<Self>().grain.is_none() {
            let grain = Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/chrome-grain.png").to_vec(),
            )
            .to_image_data(cx.svg_renderer())
            .expect("bundled chrome grain must decode");
            cx.global_mut::<Self>().grain = Some(grain);
        }
        window.set_background_appearance(if glacier(cx) && translucent {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Opaque
        });
    }

    /// Only the chrome is translucent. Tables, controls and dialogs retain
    /// their solid/gradient theme tokens; Linux and Reduce Transparency are opaque.
    pub fn background(cx: &App) -> Hsla {
        let mut color = cx.theme().title_bar;
        if cx.global::<Self>().translucent {
            color.a = 0.88;
        }
        color
    }

    pub fn grain(cx: &App) -> Canvas<()> {
        let chrome = cx.global::<Self>();
        let image = chrome.translucent.then(|| chrome.grain.clone()).flatten();
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                let Some(image) = image else { return };
                const TILE: f32 = 128.;
                const MAX_GRAIN_TILES: usize = 1024;
                let columns = (f32::from(bounds.size.width) / TILE).ceil() as usize;
                let rows = (f32::from(bounds.size.height) / TILE).ceil() as usize;
                // Decoration may disappear in an enormous window; it must never
                // turn a resize into unbounded scene work.
                if columns.saturating_mul(rows) > MAX_GRAIN_TILES {
                    return;
                }
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for y in 0..rows {
                        for x in 0..columns {
                            let tile = Bounds::new(
                                bounds.origin + point(px(x as f32 * TILE), px(y as f32 * TILE)),
                                size(px(TILE), px(TILE)),
                            );
                            let _ = window.paint_image(
                                tile,
                                tile,
                                Default::default(),
                                image.clone(),
                                0,
                                false,
                            );
                        }
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}

// Glacier's geometry (the elements spec).
/// The gutter between the window edge and the sheet, and between the sheet
/// and the footer (Oliver, 2026-10-20: 16 was "wasting of space").
pub const SHEET_GUTTER: f32 = 8.;
/// The sheet's top gutter, below the tools bar.
pub const SHEET_GUTTER_TOP: f32 = 8.;
/// The sheet's corner radius.
pub const SHEET_RADIUS: f32 = 12.;
/// The sheet's boundary.
pub const SHEET_BORDER: f32 = 1.;
/// The padding inside the boundary, on all four sides. GPUI clips to a
/// rectangle, so a column header, section band, hover or selection fill
/// that reached the sheet's edge would paint square over the rounded
/// corner. A rectangle inset by at least `SHEET_RADIUS · (1 − 1/√2)` ≈ 3.5 px
/// stays inside the rounded boundary everywhere, so nothing can poke out.
pub const SHEET_PAD: f32 = 4.;
/// What the sheet takes from the table's width: both gutters, both
/// boundary lines and both paddings.
pub const SHEET_INSET: f32 = 2. * (SHEET_GUTTER + SHEET_BORDER + SHEET_PAD);
/// A status or count capsule: height, radius, side padding, its dot and
/// the gap between dot and word.
pub const CAPSULE_HEIGHT: f32 = 20.;
pub const CAPSULE_RADIUS: f32 = 5.;
pub const CAPSULE_PAD_X: f32 = 5.;
pub const CAPSULE_DOT: f32 = 6.;
pub const CAPSULE_GAP: f32 = 5.;
/// The rail on a section header and on the selected row.
pub const RAIL: f32 = 3.;
/// A section header's rail stops this short of the row's top and bottom,
/// rounded, so a header scrolled under the column labels leaves no sliver
/// of its tone; the selection rail runs the row's full height, so a
/// multi-selection reads as one continuous bar.
pub const RAIL_INSET: f32 = 4.;
/// A control's height and radius.
pub const CONTROL_HEIGHT: f32 = 28.;
pub const CONTROL_RADIUS: f32 = 7.;

/// A capsule's fill: its tone over the sheet.
pub fn capsule_bg(tone: Hsla, cx: &App) -> Hsla {
    let theme = cx.theme();
    crate::design::tint(tone, theme.background, crate::design::CAPSULE_TINT)
}

/// A section header's band: its tone over the sheet.
pub fn band_bg(tone: Hsla, cx: &App) -> Hsla {
    let theme = cx.theme();
    crate::design::tint(tone, theme.background, crate::design::BAND_TINT)
}

/// A quick filter that is on: the link colour over the canvas, strong enough
/// to read as filled at a glance; the words on it keep the text's ink.
pub fn active_bg(cx: &App) -> Hsla {
    active_fill(cx.theme().title_bar, cx)
}

/// A control that is on, over `surface`: the one "on" recipe's fill
/// (critique C1 — the current repository, Pinned, a quick filter and a
/// Settings segment all say "on" the same way).
pub fn active_fill(surface: Hsla, cx: &App) -> Hsla {
    crate::design::tint(cx.theme().link, surface, crate::design::ACTIVE_TINT)
}

/// A label chip's fill and its edge in Glacier: the text's ink over the
/// sheet, opaque, so the chip stays on a hovered or selected row.
pub fn chip_bg(cx: &App) -> Hsla {
    let theme = cx.theme();
    crate::design::tint(theme.foreground, theme.background, crate::design::CHIP_TINT)
}

pub fn chip_edge(cx: &App) -> Hsla {
    let theme = cx.theme();
    crate::design::tint(
        theme.foreground,
        theme.background,
        crate::design::CHIP_EDGE_TINT,
    )
}

/// The selected view tab's fill: the sheet's colour in the light; in the dark
/// a lift of the text's ink over the track, so it reads raised.
pub fn raised_tab_bg(cx: &App) -> Hsla {
    let theme = cx.theme();
    if theme.mode.is_dark() {
        crate::design::tint(
            theme.foreground,
            theme.tab_bar_segmented,
            crate::design::TAB_RAISE_TINT,
        )
    } else {
        theme.background
    }
}

/// Whether the colours are a set other than Classic (Studio, Mineral or a
/// theme file), whose `input` boundary is strong enough for a text field but
/// loud on a button.
pub fn colour_set(cx: &App) -> bool {
    cx.try_global::<Look>()
        .is_some_and(|look| look.colors != Colors::Classic)
}

/// A Default-variant button at rest takes the palette's quiet `border` in the
/// colour sets rather than the text-field `input` boundary gpui-component
/// gives it (critique C7); Classic keeps the framework's. The hover and
/// pressed states are the framework's own.
pub fn quiet<B: gpui::Styled>(button: B, cx: &App) -> B {
    if colour_set(cx) {
        let border = cx.theme().border;
        button.border_color(border)
    } else {
        button
    }
}

/// The "on" recipe for a Glacier toolbar control: a ghost at rest, and when
/// on the link's tint under the text's own ink, a step bolder.
pub fn on_control(
    button: gpui_component::button::Button,
    on: bool,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::ButtonVariants;
    let theme = cx.theme();
    let button = button.ghost();
    if on {
        button
            .bg(active_bg(cx))
            .text_color(theme.foreground)
            .font_weight(gpui::FontWeight::SEMIBOLD)
    } else {
        button
    }
}

/// The selected segment of a segmented control (Settings), in every look:
/// the link's tint, a 1 px link edge and the text's ink a step bolder;
/// the others muted (critique S0 — gpui-component's own selected style is a
/// framework grey that measured 1.07:1 against its neighbours).
pub fn segment(
    button: gpui_component::button::Button,
    selected: bool,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::Selectable;
    let theme = cx.theme();
    let button = button.selected(selected);
    if selected {
        button
            .border_1()
            .border_color(theme.link)
            .bg(active_fill(theme.background, cx))
            .text_color(theme.foreground)
            .font_weight(gpui::FontWeight::SEMIBOLD)
    } else {
        button
            .text_color(theme.muted_foreground)
            .font_weight(gpui::FontWeight::MEDIUM)
    }
}

fn shadow(y: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow::new(px(0.), px(y), gpui::hsla(0., 0., 0., alpha)).blur_radius(px(blur))
}

/// The sheet's shadow: lifted off the window, a touch more in the dark.
pub fn sheet_shadow(cx: &App) -> Vec<BoxShadow> {
    if cx.theme().mode.is_dark() {
        vec![shadow(10., 28., 0.33), shadow(1., 3., 0.27)]
    } else {
        vec![shadow(8., 24., 0.10), shadow(1., 3., 0.06)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_padding_keeps_every_rectangle_inside_the_rounded_corner() {
        // The farthest point of a radius-r arc from the two edges it joins
        // lies at r·(1 − 1/√2) from each; a rectangle inset that far is
        // inside the rounded rectangle at every corner.
        let corner = SHEET_RADIUS * (1. - 1. / 2f32.sqrt());
        assert!(SHEET_PAD >= corner, "{SHEET_PAD} < {corner}");
        assert_eq!(SHEET_INSET, 2. * (SHEET_GUTTER + SHEET_BORDER + SHEET_PAD));
    }
}
