use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::render::font::{Font, FontManager, FontToken};
use crate::surface::Surface;

/// MiSans Normal, the gallery's CJK face. Shared with demos that need full
/// ASCII punctuation, such as Iris readouts.
pub(in crate::gallery) const FONT_BYTES: &[u8] = include_bytes!("../demos/assets/play_ui.mirx");
/// Geist Regular (OFL, see `assets/licenses/UI-Geist-OFL.txt`), Latin only:
/// the samples' face for words and figures, with MiSans behind it.
pub(in crate::gallery) const GEIST_BYTES: &[u8] = include_bytes!("../demos/assets/ui_geist.mirx");
/// Fusion Pixel 10 px (OFL, see `assets/licenses/Play-Fusion-Pixel-OFL.txt`),
/// baked at 10, 20 and 30 px: the face of the gallery games.
pub(in crate::gallery) const PIXEL_FONT_BYTES: &[u8] =
    include_bytes!("../demos/assets/play_pixel.mirx");
/// The pixel grid of the game face: it draws only at multiples of this size.
const GRID: u16 = 10;
#[cfg(test)]
const FONT_CHARSET: &str = include_str!("../demos/assets/play_charset.txt");

/// Registers the gallery UI typeface for every semantic font role used by
/// the gallery samples: Geist for Latin text and figures, backed by MiSans
/// for Chinese and every other glyph Geist lacks.
///
/// **Inserts**
/// - resource: `FontManager` static entries for `Default`, `Heading`, `Mono`
///   and `FontToken::FALLBACK`
#[derive(Default)]
pub(in crate::gallery) struct PlayFontPlugin;

impl<B, F> Plugin<B, F> for PlayFontPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let limits = &mirx::reader::PayloadLimits::HOST;
        let geist = Font::from_mirx("MIRUI Play", 12, GEIST_BYTES, limits)
            .expect("MIRUI Play font must decode");
        let misans = Font::from_mirx("MIRUI Play CJK", 12, FONT_BYTES, limits)
            .expect("MIRUI Play CJK font must decode");
        register_every_role(app, geist);
        if let Some(manager) = app.world.resource::<FontManager>() {
            manager.add_static(FontToken::FALLBACK.cache_key(), misans);
        }
    }
}

/// Registers the pixel game face for every semantic font role used by the
/// gallery games.
///
/// **Inserts**
/// - resource: `FontManager` static entries for `Default`, `Heading`, and `Mono`
#[derive(Default)]
pub(in crate::gallery) struct GameFontPlugin;

impl<B, F> Plugin<B, F> for GameFontPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let font = Font::from_mirx(
            "MIRUI Pixel",
            GRID,
            PIXEL_FONT_BYTES,
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("MIRUI Pixel font must decode")
        .with_pixel_grid(GRID);
        register_every_role(app, font);
    }
}

fn register_every_role<B, F>(app: &mut App<B, F>, font: Font)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let Some(manager) = app.world.resource::<FontManager>() else {
        return;
    };
    manager.add_static(FontToken::Default.cache_key(), font.clone());
    manager.add_static(FontToken::Heading.cache_key(), font.clone());
    manager.add_static(FontToken::Mono.cache_key(), font);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::FontProvider;
    use crate::render::font::mirx::MirxFontProvider;
    use mirx::font::FontRepresentationKind;

    const TWIN_SOURCE: &str = concat!(
        include_str!("twin/mod.rs"),
        "\n",
        include_str!("twin/chapters.rs"),
        "\n",
        include_str!("twin/types.rs"),
        "\n",
        include_str!("twin/rules.rs"),
        "\n",
        include_str!("twin/solver.rs"),
        "\n",
        include_str!("twin/model.rs"),
        "\n",
        include_str!("twin/tests.rs"),
        "\n",
        include_str!("../demos/twin_beacons.rs"),
        "\n",
        include_str!("../demos/twin_beacons/style.rs"),
        "\n",
        include_str!("../demos/twin_beacons/state.rs"),
        "\n",
        include_str!("../demos/twin_beacons/render.rs"),
        "\n",
        include_str!("../demos/twin_beacons/input.rs"),
        "\n",
        include_str!("../demos/twin_beacons/shell.rs"),
        "\n",
        include_str!("../demos/twin_beacons/tests.rs"),
    );

    const FOLD_SOURCE: &str = concat!(
        include_str!("fold/mod.rs"),
        "\n",
        include_str!("fold/chapters.rs"),
        "\n",
        include_str!("fold/types.rs"),
        "\n",
        include_str!("fold/rules.rs"),
        "\n",
        include_str!("fold/solver.rs"),
        "\n",
        include_str!("fold/model.rs"),
        "\n",
        include_str!("fold/tests.rs"),
        "\n",
        include_str!("../demos/folding_ark.rs"),
        "\n",
        include_str!("../demos/folding_ark/style.rs"),
        "\n",
        include_str!("../demos/folding_ark/state.rs"),
        "\n",
        include_str!("../demos/folding_ark/render.rs"),
        "\n",
        include_str!("../demos/folding_ark/input.rs"),
        "\n",
        include_str!("../demos/folding_ark/shell.rs"),
        "\n",
        include_str!("../demos/folding_ark/tests.rs"),
    );

    const PICTURE_SOURCE: &str = concat!(
        include_str!("picture/mod.rs"),
        "\n",
        include_str!("picture/chapters.rs"),
        "\n",
        include_str!("picture/types.rs"),
        "\n",
        include_str!("picture/model.rs"),
        "\n",
        include_str!("picture/tests.rs"),
        "\n",
        include_str!("../demos/atlas_restoration.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/style.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/geometry.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/state.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/render.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/input.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/persistence.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/shell.rs"),
        "\n",
        include_str!("../demos/atlas_restoration/tests.rs"),
    );

    /// UI sources of the other games. Only their string literals are
    /// checked, so comments may use any character.
    const GAME_SOURCES: &[&str] = &[
        include_str!("expeditions/words.rs"),
        include_str!("../demos/marble_play.rs"),
        include_str!("../demos/marble_play/inspector.rs"),
        include_str!("../demos/marble_play/scenes.rs"),
        include_str!("../demos/marble_play/settings.rs"),
        include_str!("../demos/marble_play/shell.rs"),
        include_str!("../demos/marble_play/audio.rs"),
        include_str!("marble/model.rs"),
        include_str!("marble/physics.rs"),
        include_str!("marble/presets.rs"),
        include_str!("marble/transport.rs"),
        include_str!("marble/types.rs"),
        include_str!("../demos/lumen_lab.rs"),
        include_str!("../demos/lumen_lab/shell.rs"),
        include_str!("lumen/types.rs"),
        include_str!("lumen/levels.rs"),
        include_str!("../demos/pixel_loom/render.rs"),
        include_str!("../demos/pixel_loom/shell.rs"),
        include_str!("../demos/pixel_loom/style.rs"),
        include_str!("pixel/types.rs"),
        include_str!("../demos/moss_study/render.rs"),
        include_str!("../demos/moss_study/shell.rs"),
        include_str!("../demos/pocket_post/render.rs"),
        include_str!("../demos/pocket_post/shell.rs"),
        include_str!("post/data.rs"),
        include_str!("../demos/logic_circuit/render.rs"),
        include_str!("../demos/logic_circuit/shell.rs"),
        include_str!("circuit/tasks.rs"),
        include_str!("circuit/types.rs"),
        include_str!("../demos/module_factory/render.rs"),
        include_str!("../demos/module_factory/shell.rs"),
        include_str!("../demos/module_factory/text.rs"),
        include_str!("factory/missions.rs"),
        include_str!("factory/types.rs"),
        include_str!("../demos/orbital_mission/render.rs"),
        include_str!("../demos/orbital_mission/shell.rs"),
        include_str!("orbit/missions.rs"),
        include_str!("../demos/tidal_atlas/render.rs"),
        include_str!("../demos/tidal_atlas/shell.rs"),
        include_str!("tidal/goals.rs"),
        include_str!("tidal/types.rs"),
        include_str!("../demos/echo_walker/render.rs"),
        include_str!("../demos/echo_walker/shell.rs"),
    ];

    /// Yields the characters inside the string literals of `source`,
    /// skipping line comments and character literals.
    fn literal_chars(source: &str) -> impl Iterator<Item = char> + '_ {
        source.lines().flat_map(|line| {
            let mut inside = false;
            let mut escaped = false;
            let mut previous = ' ';
            line.chars().filter(move |&scalar| {
                let keep = if inside {
                    if escaped {
                        escaped = false;
                        false
                    } else if scalar == '\\' {
                        escaped = true;
                        false
                    } else if scalar == '"' {
                        inside = false;
                        false
                    } else {
                        true
                    }
                } else {
                    if scalar == '/' && previous == '/' {
                        inside = false;
                        previous = '\n';
                        return false;
                    }
                    if scalar == '"' && previous != '\'' && previous != '\n' {
                        inside = true;
                    }
                    false
                };
                if previous != '\n' {
                    previous = scalar;
                }
                keep
            })
        })
    }

    #[test]
    fn play_font_draws_every_game_string() {
        let provider =
            MirxFontProvider::from_mirx(PIXEL_FONT_BYTES, &mirx::reader::PayloadLimits::HOST)
                .unwrap();
        for (index, source) in GAME_SOURCES.iter().enumerate() {
            for scalar in literal_chars(source).filter(|scalar| !scalar.is_whitespace()) {
                assert!(
                    provider.map_char(scalar).is_some(),
                    "game source {index} uses {scalar:?}, which the play font lacks"
                );
            }
        }
    }

    #[test]
    fn plugins_register_every_font_role() {
        let mut samples = App::headless(32, 32);
        samples.add_plugin(PlayFontPlugin);
        let mut games = App::headless(32, 32);
        games.add_plugin(GameFontPlugin);

        let samples = samples.world.resource::<FontManager>().unwrap();
        let games = games.world.resource::<FontManager>().unwrap();
        for token in [FontToken::Default, FontToken::Heading, FontToken::Mono] {
            let font = samples.resolve(token.cache_key());
            assert_eq!(font.family, "MIRUI Play");
            assert_eq!(font.size, 12);
            let font = games.resolve(token.cache_key());
            assert_eq!(font.family, "MIRUI Pixel");
            assert_eq!(font.size, GRID);
        }
        let backing = samples.resolve(FontToken::FALLBACK.cache_key());
        assert_eq!(backing.family, "MIRUI Play CJK");
        assert!(backing.map_char('光').is_some());
        let geist = samples.resolve(FontToken::Default.cache_key());
        assert!(geist.map_char('g').is_some() && geist.map_char('光').is_none());
    }

    #[test]
    fn play_font_covers_declared_charset_and_small_sizes() {
        let provider =
            MirxFontProvider::from_mirx(PIXEL_FONT_BYTES, &mirx::reader::PayloadLimits::HOST)
                .unwrap();
        for scalar in FONT_CHARSET
            .chars()
            .filter(|scalar| !scalar.is_whitespace())
        {
            assert!(provider.map_char(scalar).is_some(), "missing {scalar}");
        }
        for source in [TWIN_SOURCE, FOLD_SOURCE, PICTURE_SOURCE] {
            for scalar in source
                .chars()
                .filter(|scalar| !scalar.is_ascii() && !scalar.is_whitespace())
            {
                assert!(provider.map_char(scalar).is_some(), "missing {scalar}");
            }
        }
        let glyph = provider.map_char('光').unwrap();
        for size in [10, 20, 30] {
            let raster = provider.raster(glyph, size, size).unwrap();
            assert_eq!(
                raster.representation.kind(),
                FontRepresentationKind::Coverage { bits: 1 }
            );
            assert_eq!(raster.representation.design_ppem(), size);
        }
    }

    #[test]
    fn play_font_draws_every_size_from_a_whole_strike() {
        let mut app = App::headless(32, 32);
        app.add_plugin(GameFontPlugin);
        let manager = app.world.resource::<FontManager>().unwrap();
        let font = manager.resolve(FontToken::Default.cache_key());
        let glyph = font.map_char('光').unwrap();
        for (asked, drawn) in [(7, 10), (9, 10), (13, 10), (18, 20), (34, 30)] {
            assert_eq!(font.ppem_for(Some(asked)), drawn);
            let raster = font.raster(glyph, asked).unwrap();
            assert_eq!(raster.representation.design_ppem(), drawn);
            assert_eq!(
                font.glyph_advance(glyph, asked),
                font.glyph_advance(glyph, drawn)
            );
        }
    }
}
