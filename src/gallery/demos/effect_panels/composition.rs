use super::animation::{BlurPan, ColorFlash, GlowPulse, ShadowOffset};
use super::style::tile_color;
use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{
    BackgroundBlur, DropGlow, DropShadow, MirrorOf, ParagraphStyle, TemporalMix, Text, TextAlign,
    WidgetTransform,
};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(10),
            row_gap: 8,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "LIVE EFFECT PIPELINE",
                width: Dimension::percent(100),
                max_width: 900,
                height: 28,
                font_size: 16,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "effect_panel_grid",
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 900,
                wrap: FlexWrap::Wrap,
                align: AlignItems::Stretch,
                row_gap: 8,
                column_gap: 8
            ) {
                Column (
                    id: "effect_mirror_card",
                    width: Dimension::percent(48),
                    height: Dimension::percent(46),
                    padding: Padding::all(8),
                    row_gap: 6,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 12
                ) {
                    Text (
                        "MIRROR",
                        height: 14,
                        font_size: 9,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (grow: 1.0, align: AlignItems::Center, column_gap: 8) {
                        Text (
                            "SOURCE",
                            id: "mirror-src",
                            bg_color: ColorToken::Primary,
                            text_color: ColorToken::OnPrimary,
                            border_radius: 8,
                            grow: 1.0,
                            height: Dimension::percent(100),
                            max_height: 96,
                            font_size: 9,
                            paragraph: ParagraphStyle::label()
                        )
                        View (
                            grow: 1.0,
                            height: Dimension::percent(100),
                            max_height: 96,
                            border_radius: 8,
                            clip_children: true
                        ) [
                            MirrorOf::new(id("mirror-src")).with_fade(160),
                        ]
                    }
                }
                Column (
                    id: "effect_temporal_card",
                    width: Dimension::percent(48),
                    height: Dimension::percent(46),
                    padding: Padding::all(8),
                    row_gap: 6,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 12
                ) {
                    Text (
                        "TEMPORAL MIX",
                        height: 14,
                        font_size: 9,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (grow: 1.0, align: AlignItems::Center, column_gap: 8) {
                        View (
                            id: "tm_src",
                            grow: 1.0,
                            height: Dimension::percent(100),
                            max_height: 96,
                            border_radius: 8
                        ) [
                            ColorFlash { frame: 0 },
                        ]
                        View (
                            grow: 1.0,
                            height: Dimension::percent(100),
                            max_height: 96,
                            border_radius: 8,
                            clip_children: true
                        ) [
                            TemporalMix::new(id("tm_src")).with_mix(230),
                        ]
                    }
                }
                Column (
                    id: "effect_blur_card",
                    width: Dimension::percent(48),
                    height: Dimension::percent(46),
                    padding: Padding::all(8),
                    row_gap: 6,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 12
                ) {
                    Text (
                        "BACKGROUND BLUR",
                        height: 14,
                        font_size: 9,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Column (
                        id: "effect_blur_stage",
                        grow: 1.0,
                        border_radius: 8,
                        clip_children: true
                    ) {
                        walk 0..3i32 with i {
                            Row (grow: 1.0) {
                                walk 0..4i32 with j {
                                    View (grow: 1.0, bg_color: tile_color(j + i * 4))
                                }
                            }
                        }
                        View (
                            id: "effect_blur_overlay",
                            position: Position::Absolute,
                            left: 0,
                            top: 8,
                            width: Dimension::percent(58),
                            height: Dimension::percent(64),
                            bg_color: Color::rgba(255, 255, 255, 50),
                            border_radius: 8
                        ) [
                            BackgroundBlur::new(8),
                            WidgetTransform::default(),
                            BlurPan(
                                Tween::new(
                                        Fixed::ZERO,
                                        Fixed::ONE,
                                        2200,
                                        ease::ease_in_out_cubic,
                                        PlayMode::PingPong,
                                    )
                                    .into(),
                            ),
                        ]
                    }
                }
                Column (
                    id: "effect_light_card",
                    width: Dimension::percent(48),
                    height: Dimension::percent(46),
                    padding: Padding::all(8),
                    row_gap: 6,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 12
                ) {
                    Text (
                        "SHADOW + GLOW",
                        height: 14,
                        font_size: 9,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (grow: 1.0, column_gap: 10) {
                        View (grow: 1.0) {
                            View (
                                id: "effect_shadow_fx",
                                position: Position::Absolute,
                                left: Dimension::percent(15),
                                top: Dimension::percent(22),
                                width: Dimension::percent(70),
                                height: Dimension::percent(56)
                            )
                            View (
                                id: "effect_shadow_src",
                                position: Position::Absolute,
                                left: Dimension::percent(15),
                                top: Dimension::percent(22),
                                width: Dimension::percent(70),
                                height: Dimension::percent(56),
                                bg_color: ColorToken::Primary,
                                border_radius: 10
                            )
                        }
                        View (grow: 1.0) {
                            View (
                                id: "effect_glow_fx",
                                position: Position::Absolute,
                                left: Dimension::percent(15),
                                top: Dimension::percent(22),
                                width: Dimension::percent(70),
                                height: Dimension::percent(56)
                            )
                            View (
                                id: "effect_glow_src",
                                position: Position::Absolute,
                                left: Dimension::percent(15),
                                top: Dimension::percent(22),
                                width: Dimension::percent(70),
                                height: Dimension::percent(56),
                                bg_color: ColorToken::Secondary,
                                border_radius: 10
                            )
                        }
                    }
                }
            }
        }
    };

    let sh_fx = cx
        .world_mut()
        .find_by_id("effect_shadow_fx")
        .expect("shadow effect");
    let sh_src = cx
        .world_mut()
        .find_by_id("effect_shadow_src")
        .expect("shadow source");
    cx.world_mut().insert(
        sh_fx,
        DropShadow::new(sh_src)
            .with_blur_radius(6)
            .with_offset(4, 4)
            .with_color(ColorToken::Shadow)
            .with_opacity(160),
    );
    cx.world_mut().insert(
        sh_fx,
        ShadowOffset(
            Tween::new(
                Fixed::from_int(2),
                Fixed::from_int(8),
                1500,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );

    let gl_fx = cx
        .world_mut()
        .find_by_id("effect_glow_fx")
        .expect("glow effect");
    let gl_src = cx
        .world_mut()
        .find_by_id("effect_glow_src")
        .expect("glow source");
    cx.world_mut().insert(
        gl_fx,
        DropGlow::new(gl_src)
            .with_blur_radius(12)
            .with_color(ColorToken::Primary)
            .with_opacity(180),
    );
    cx.world_mut().insert(
        gl_fx,
        GlowPulse(
            Tween::new(
                Fixed::from_int(6),
                Fixed::from_int(18),
                1800,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );
}
