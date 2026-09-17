extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{
    BackgroundBlur, DropGlow, DropShadow, MirrorOf, ParagraphStyle, TemporalMix, Text, TextAlign,
    WidgetTransform,
};
use crate::ui::{ComputedRect, Parent};

pub const DEFAULT_VIEW: (u16, u16) = (360, 560);

pub struct ColorFlash {
    pub frame: u32,
}

#[system(order = ANIMATION)]
pub fn animate_color_flash(world: &mut World) {
    world.for_each_stable::<ColorFlash>(|world, e| {
        let frame = match world.get_mut::<ColorFlash>(e) {
            Some(c) => {
                c.frame = c.frame.wrapping_add(1);
                c.frame
            }
            None => return,
        };
        let color = match (frame / 60) % 3 {
            0 => Color::rgb(220, 60, 60),
            1 => Color::rgb(60, 200, 80),
            _ => Color::rgb(40, 140, 220),
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.bg_color = Some(color.into());
        }
        world.invalidate(e);
    });
}

animate!(BlurPan, |world, entity, value| {
    let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.0) else {
        return;
    };
    let Some(stage) = world.get::<ComputedRect>(parent).map(|rect| rect.0) else {
        return;
    };
    let Some(overlay) = world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
        return;
    };
    let travel = (stage.w - overlay.w).max(Fixed::ZERO);
    if let Some(transform) = world.get_mut::<WidgetTransform>(entity) {
        transform.0.tx = travel * value;
    }
    world.invalidate_visual(entity);
});

animate!(ShadowOffset, |world, entity, value| {
    if let Some(sh) = world.get_mut::<DropShadow>(entity) {
        sh.offset.0 = value;
        sh.offset.1 = value;
    }
    world.invalidate(entity);
});

animate!(GlowPulse, |world, entity, value| {
    if let Some(gl) = world.get_mut::<DropGlow>(entity) {
        gl.blur_radius = value;
    }
    world.invalidate(entity);
});

fn tile_color(i: i32) -> Color {
    [
        Color::rgb(220, 60, 60),
        Color::rgb(220, 160, 40),
        Color::rgb(60, 200, 80),
        Color::rgb(40, 140, 220),
        Color::rgb(180, 80, 220),
        Color::rgb(40, 200, 200),
    ][(i % 6) as usize]
}

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

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin)
        .with_offscreen_pool_budget(1024 * 1024)
        .add_system(animate_color_flash::system())
        .add_system(BlurPan::system())
        .add_system(ShadowOffset::system())
        .add_system(GlowPulse::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::DeltaTimeMs;
    use crate::types::Viewport;
    use crate::ui::render_system::update_layout;
    use crate::ui::{Children, IdMap, Theme, UiScope};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn cards_stay_inside_portrait_landscape_and_desktop_viewports() {
        for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, build_widgets);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            for id in [
                "effect_mirror_card",
                "effect_temporal_card",
                "effect_blur_card",
                "effect_light_card",
            ] {
                let entity = app.world.find_by_id(id).unwrap();
                let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(rect.x >= Fixed::ZERO, "{id} starts before the viewport");
                assert!(rect.y >= Fixed::ZERO, "{id} starts above the viewport");
                assert!(
                    rect.x + rect.w <= Fixed::from_int(width as i32),
                    "{id} exceeds the viewport width at {width}x{height}",
                );
                assert!(
                    rect.y + rect.h <= Fixed::from_int(height as i32),
                    "{id} exceeds the viewport height at {width}x{height}",
                );
            }
        }
    }

    #[test]
    fn setup_keeps_the_callers_active_theme() {
        let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
        app.with_default_widgets()
            .with_default_systems()
            .with_theme(Theme::light());
        let root = app.spawn_root().id();
        setup_app(&mut app, root);

        assert_eq!(
            app.world
                .resource::<Theme>()
                .unwrap()
                .resolve(ColorToken::Surface),
            Theme::light().resolve(ColorToken::Surface),
        );
    }

    #[test]
    fn blur_motion_updates_only_the_visual_transform() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets);
        app.set_root(root);
        update_layout(&mut app.world, root, &Viewport::new(480, 320, Fixed::ONE));

        let overlay = app.world.find_by_id("effect_blur_overlay").unwrap();
        let left_before = app.world.get::<Style>(overlay).unwrap().layout.left;
        app.world.insert_resource(DeltaTimeMs(1_100));
        (BlurPan::system().run)(&mut app.world);

        assert!(
            app.world.get::<WidgetTransform>(overlay).unwrap().0.tx > Fixed::ZERO,
            "blur overlay must travel inside its live stage",
        );
        assert_eq!(
            app.world.get::<Style>(overlay).unwrap().layout.left,
            left_before,
            "visual motion must not dirty layout geometry",
        );
    }
}
