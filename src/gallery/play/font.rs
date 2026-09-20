use crate::ecs::World;
use crate::render::font::{Font, FontManager, FontToken};

const FONT_BYTES: &[u8] = include_bytes!("../demos/assets/play_ui.mirx");

pub(crate) fn register_play_font(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::FontProvider;
    use crate::render::font::mirx::MirxFontProvider;
    use mirx::font::FontRepresentationKind;

    #[test]
    fn play_font_covers_lumen_labels_and_small_sizes() {
        let provider =
            MirxFontProvider::from_mirx(FONT_BYTES, &mirx::reader::PayloadLimits::HOST).unwrap();
        for scalar in
            "第一束光折返航线绕过岛屿交错最后公里提示重新摆放追撤销关卡镜片方向切换等待点亮次旋转已"
                .chars()
        {
            assert!(provider.map_char(scalar).is_some(), "missing {scalar}");
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
