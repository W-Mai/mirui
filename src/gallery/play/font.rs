use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::render::font::{Font, FontManager, FontToken};
use crate::surface::Surface;

const FONT_BYTES: &[u8] = include_bytes!("../demos/assets/play_ui.mirx");
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
            12,
            FONT_BYTES,
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("MIRUI Play font must decode");
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

    #[test]
    fn plugin_registers_every_play_font_role() {
        let mut app = App::headless(32, 32);
        app.add_plugin(PlayFontPlugin);

        let manager = app.world.resource::<FontManager>().unwrap();
        for token in [FontToken::Default, FontToken::Heading, FontToken::Mono] {
            let font = manager.resolve(token.cache_key());
            assert_eq!(font.family, "MIRUI Play");
            assert_eq!(font.size, 12);
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
        for size in [10, 12] {
            let raster = provider.raster(glyph, size, size).unwrap();
            assert_eq!(
                raster.representation.kind(),
                FontRepresentationKind::Coverage { bits: 8 }
            );
            assert_eq!(raster.representation.design_ppem(), size);
        }
    }
}
