use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::render::font::{Font, FontManager, FontToken};
use crate::surface::Surface;

/// Fusion Pixel 10 px (OFL, see `assets/licenses/Play-Fusion-Pixel-OFL.txt`),
/// baked at 10, 20 and 30 px. Shared with demos that need full ASCII
/// punctuation, such as Iris readouts.
pub(in crate::gallery) const FONT_BYTES: &[u8] = include_bytes!("../demos/assets/play_pixel.mirx");
/// The pixel grid of the face: it draws only at multiples of this size.
const GRID: u16 = 10;
#[cfg(test)]
const FONT_CHARSET: &str = include_str!("../demos/assets/play_charset.txt");

/// Registers the bundled play typeface for every semantic font role used by
/// the gallery games.
///
/// **Inserts**
/// - resource: `FontManager` static entries for `Default`, `Heading`, and `Mono`
#[derive(Default)]
pub(in crate::gallery) struct PlayFontPlugin;

impl<B, F> Plugin<B, F> for PlayFontPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let Some(manager) = app.world.resource::<FontManager>() else {
            return;
        };
        let base = Font::from_mirx(
            "MIRUI Play",
            GRID,
            FONT_BYTES,
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("MIRUI Play font must decode")
        .with_pixel_grid(GRID);
        manager.add_static(FontToken::Default.cache_key(), base.clone());
        manager.add_static(FontToken::Heading.cache_key(), base.clone());
        manager.add_static(FontToken::Mono.cache_key(), base);
    }
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
        include_str!("../demos/marble_play.rs"),
        include_str!("../demos/marble_play/inspector.rs"),
        include_str!("../demos/marble_play/scenes.rs"),
        include_str!("../demos/marble_play/settings.rs"),
        include_str!("../demos/marble_play/shell.rs"),
        include_str!("../demos/lumen_lab.rs"),
        include_str!("../demos/lumen_lab/shell.rs"),
        include_str!("lumen/types.rs"),
        include_str!("../demos/pixel_loom/render.rs"),
        include_str!("../demos/pixel_loom/shell.rs"),
        include_str!("../demos/moss_study/render.rs"),
        include_str!("../demos/moss_study/shell.rs"),
        include_str!("../demos/pocket_post/render.rs"),
        include_str!("../demos/pocket_post/shell.rs"),
        include_str!("../demos/logic_circuit/render.rs"),
        include_str!("../demos/logic_circuit/shell.rs"),
        include_str!("../demos/module_factory/render.rs"),
        include_str!("../demos/module_factory/shell.rs"),
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
            MirxFontProvider::from_mirx(FONT_BYTES, &mirx::reader::PayloadLimits::HOST).unwrap();
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
    fn plugin_registers_every_play_font_role() {
        let mut app = App::headless(32, 32);
        app.add_plugin(PlayFontPlugin);

        let manager = app.world.resource::<FontManager>().unwrap();
        for token in [FontToken::Default, FontToken::Heading, FontToken::Mono] {
            let font = manager.resolve(token.cache_key());
            assert_eq!(font.family, "MIRUI Play");
            assert_eq!(font.size, GRID);
        }
    }

    #[test]
    fn play_font_covers_declared_charset_and_small_sizes() {
        let provider =
            MirxFontProvider::from_mirx(FONT_BYTES, &mirx::reader::PayloadLimits::HOST).unwrap();
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
        app.add_plugin(PlayFontPlugin);
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
