use crate::ecs::{Entity, World};
use crate::render::PlaneLayout;
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::types::{Color, Fixed, Fixed64, Point, Rect};
use crate::ui::offscreen::{WidgetTexture, WidgetTextureRead};
use crate::ui::outputs::Outputs;
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{PaintCtx, PaintMeasurement, SourceGeometry, View};

#[crate::component]
pub struct DropShadow {
    #[depends_on(WidgetTexture)]
    pub source: Entity,
    pub blur_radius: Fixed,
    pub offset: (Fixed, Fixed),
    pub color: ThemedColor,
    pub opacity: u8,
}

impl DropShadow {
    pub fn new(source: Entity) -> Self {
        Self {
            source,
            blur_radius: Fixed::from_int(4),
            offset: (Fixed::from_int(2), Fixed::from_int(2)),
            color: ThemedColor::Token(ColorToken::Shadow),
            opacity: 180,
        }
    }

    pub fn with_blur_radius(mut self, r: impl Into<Fixed>) -> Self {
        self.blur_radius = r.into();
        self
    }

    pub fn with_offset(mut self, x: impl Into<Fixed>, y: impl Into<Fixed>) -> Self {
        self.offset = (x.into(), y.into());
        self
    }

    pub fn with_color(mut self, c: impl Into<ThemedColor>) -> Self {
        self.color = c.into();
        self
    }

    pub fn with_opacity(mut self, o: u8) -> Self {
        self.opacity = o;
        self
    }
}

#[crate::view(component = DropShadow, name = "DropShadow", priority = 60)]
fn drop_shadow_render(
    shadow: component!(),
    outputs: res!(Outputs<WidgetTexture>),
    theme: res!(crate::ui::Theme),
    paint: paint!(),
) {
    let Some(source) = outputs.get(shadow.source) else {
        return;
    };
    render_drop_effect(
        &source,
        shadow.blur_radius,
        shadow.offset,
        shadow.color.resolve(&theme),
        shadow.opacity,
        CompositeMode::SourceOver,
        paint,
    );
}

pub fn view() -> View {
    drop_shadow_render::view().with_measurement(shadow_measurement)
}

fn shadow_measurement(
    world: &World,
    entity: Entity,
    _rect: Rect,
    source_geometry: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<PaintMeasurement> {
    let shadow = world.get::<DropShadow>(entity)?;
    let (plane, bounds) = effect_geometry(
        source_geometry(shadow.source)?,
        shadow.blur_radius,
        shadow.offset,
    )?;
    Some(PaintMeasurement {
        bounds: Some(bounds),
        workspace: Some(plane.layout),
    })
}

struct EffectPlane {
    layout: PlaneLayout,
    padding: (u16, u16),
    bounds: Rect,
    alpha: (Fixed, Fixed),
}

fn checked_fixed(value: Fixed64) -> Option<Fixed> {
    (value >= Fixed64::from_fixed(Fixed::MIN) && value <= Fixed64::from_fixed(Fixed::MAX))
        .then(|| value.to_fixed())
}

fn ceil_positive(value: Fixed64) -> i64 {
    let floor = value.to_int();
    floor + i64::from(value > Fixed64::from_int(floor))
}

fn effect_plane(
    source: PlaneLayout,
    bounds: Rect,
    blur_radius: Fixed,
    offset: (Fixed, Fixed),
) -> Option<EffectPlane> {
    if source.format() != ColorFormat::RGBA8888 || blur_radius <= Fixed::ZERO {
        return None;
    }
    let radius = Fixed64::from_fixed(blur_radius);
    let offset = Fixed64::from_fixed(offset.0)
        .abs()
        .max(Fixed64::from_fixed(offset.1).abs());
    let padding = Fixed64::from_int(ceil_positive(radius) + ceil_positive(offset));
    let axis = |pixels: u16, logical: Fixed| -> Option<(u16, Fixed, Fixed, Fixed)> {
        if pixels == 0 || logical <= Fixed::ZERO {
            return None;
        }
        let logical = Fixed64::from_fixed(logical);
        let pixels_fixed = Fixed64::from_int(i64::from(pixels));
        let scaled_padding = padding.mul_wide(pixels_fixed);
        let limit = Fixed64::from_int(i64::from((u16::MAX - pixels) / 2));
        // Bound the quotient before dividing, including subpixel logical sizes.
        if scaled_padding > limit.mul_wide(logical) {
            return None;
        }
        let floor = scaled_padding.div_wide(logical).to_int();
        let pad = u16::try_from(
            floor + i64::from(Fixed64::from_int(floor).mul_wide(logical) < scaled_padding),
        )
        .ok()?;
        let logical_pad = logical
            .mul_wide(Fixed64::from_int(i64::from(pad)))
            .div_wide(pixels_fixed);
        Some((
            pad,
            checked_fixed(logical_pad)?,
            checked_fixed(logical + logical_pad + logical_pad)?,
            alpha_for_radius(checked_fixed(
                radius.mul_wide(pixels_fixed).div_wide(logical),
            )?),
        ))
    };
    let (pad_x, inset_x, width, alpha_x) = axis(source.width(), bounds.w)?;
    let (pad_y, inset_y, height, alpha_y) = axis(source.height(), bounds.h)?;
    Some(EffectPlane {
        layout: PlaneLayout::packed(
            source.width().checked_add(pad_x.checked_mul(2)?)?,
            source.height().checked_add(pad_y.checked_mul(2)?)?,
            ColorFormat::RGBA8888,
        )
        .ok()?,
        padding: (pad_x, pad_y),
        bounds: Rect::new(-inset_x, -inset_y, width, height),
        alpha: (alpha_x, alpha_y),
    })
}

fn effect_geometry(
    source: SourceGeometry,
    blur_radius: Fixed,
    offset: (Fixed, Fixed),
) -> Option<(EffectPlane, Rect)> {
    let plane = effect_plane(source.plane, source.local_bounds, blur_radius, offset)?;
    let origin = |layout: Fixed, local: Fixed, offset: Fixed, inset: Fixed| {
        checked_fixed(
            Fixed64::from_fixed(layout)
                + Fixed64::from_fixed(local)
                + Fixed64::from_fixed(offset)
                + Fixed64::from_fixed(inset),
        )
    };
    let x = origin(
        source.layout_rect.x,
        source.local_bounds.x,
        offset.0,
        plane.bounds.x,
    )?;
    let y = origin(
        source.layout_rect.y,
        source.local_bounds.y,
        offset.1,
        plane.bounds.y,
    )?;
    checked_fixed(Fixed64::from_fixed(x) + Fixed64::from_fixed(plane.bounds.w))?;
    checked_fixed(Fixed64::from_fixed(y) + Fixed64::from_fixed(plane.bounds.h))?;
    let bounds = Rect::new(x, y, plane.bounds.w, plane.bounds.h);
    Some((plane, bounds))
}

fn blur_effect(texture: &mut Texture<'_>, alpha: (Fixed, Fixed)) {
    let width = texture.width;
    let height = texture.height;
    if alpha.0 == alpha.1 {
        let bounds = Rect::new(0, 0, width, height);
        iir_blur_inplace(texture, alpha.0, bounds);
    } else {
        for y in 0..height {
            iir_blur_inplace(texture, alpha.0, Rect::new(0, y, width, 1));
        }
        for x in 0..width {
            iir_blur_inplace(texture, alpha.1, Rect::new(x, 0, 1, height));
        }
    }
}

fn render_drop_effect(
    output: &WidgetTextureRead<'_>,
    blur_radius: Fixed,
    offset: (Fixed, Fixed),
    color: Color,
    opacity: u8,
    composite: CompositeMode,
    paint: &mut PaintCtx<'_>,
) {
    let src = output.texture();
    let Ok(source_layout) = PlaneLayout::packed(src.width, src.height, src.format) else {
        return;
    };
    let source = SourceGeometry {
        plane: source_layout,
        layout_rect: output.layout_rect(),
        local_bounds: output.local_bounds(),
    };
    let Some((plane, bounds)) = effect_geometry(source, blur_radius, offset) else {
        return;
    };
    let rect = paint.map_layout_rect(bounds);
    paint.with_workspace(plane.layout, |work, paint| {
        work.alpha_mode = AlphaMode::Blend;
        work.transient = true;
        let dst_stride = work.stride;
        let src_stride = src.stride;
        let buf = work.buf.as_mut_slice();
        buf.fill(0);
        for y in 0..usize::from(src.height) {
            let si = y * src_stride;
            let di =
                (y + usize::from(plane.padding.1)) * dst_stride + usize::from(plane.padding.0) * 4;
            let copy_len = usize::from(src.width) * 4;
            buf[di..di + copy_len].copy_from_slice(&src.buf.as_slice()[si..si + copy_len]);
        }
        blur_effect(work, plane.alpha);
        let buf = work.buf.as_mut_slice();
        for y in 0..usize::from(work.height) {
            for x in 0..usize::from(work.width) {
                let i = y * dst_stride + x * 4;
                let a = buf[i + 3] as u32;
                let tinted_a = (a * opacity as u32 / 255).min(255) as u8;
                buf[i] = color.r;
                buf[i + 1] = color.g;
                buf[i + 2] = color.b;
                buf[i + 3] = tinted_a;
            }
        }
        paint.draw(&DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: paint.transform(),
            quad: None,
            texture: work,
            opa: 255,
            radius: Fixed::ZERO,
            composite,
        });
    });
}

#[crate::component]
pub struct DropGlow {
    #[depends_on(WidgetTexture)]
    pub source: Entity,
    pub blur_radius: Fixed,
    pub color: ThemedColor,
    pub opacity: u8,
}

impl DropGlow {
    pub fn new(source: Entity) -> Self {
        Self {
            source,
            blur_radius: Fixed::from_int(8),
            color: ThemedColor::Token(ColorToken::Primary),
            opacity: 180,
        }
    }

    pub fn with_blur_radius(mut self, r: impl Into<Fixed>) -> Self {
        self.blur_radius = r.into();
        self
    }

    pub fn with_color(mut self, c: impl Into<ThemedColor>) -> Self {
        self.color = c.into();
        self
    }

    pub fn with_opacity(mut self, o: u8) -> Self {
        self.opacity = o;
        self
    }
}

#[crate::view(component = DropGlow, name = "DropGlow", priority = 60)]
fn drop_glow_render(
    glow: component!(),
    outputs: res!(Outputs<WidgetTexture>),
    theme: res!(crate::ui::Theme),
    paint: paint!(),
) {
    let Some(source) = outputs.get(glow.source) else {
        return;
    };
    render_drop_effect(
        &source,
        glow.blur_radius,
        (Fixed::ZERO, Fixed::ZERO),
        glow.color.resolve(&theme),
        glow.opacity,
        CompositeMode::Screen,
        paint,
    );
}

pub fn glow_view() -> View {
    drop_glow_render::view().with_measurement(glow_measurement)
}

fn glow_measurement(
    world: &World,
    entity: Entity,
    _rect: Rect,
    source_geometry: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<PaintMeasurement> {
    let glow = world.get::<DropGlow>(entity)?;
    let (plane, bounds) = effect_geometry(
        source_geometry(glow.source)?,
        glow.blur_radius,
        (Fixed::ZERO, Fixed::ZERO),
    )?;
    Some(PaintMeasurement {
        bounds: Some(bounds),
        workspace: Some(plane.layout),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::types::Transform;
    use crate::ui::widgets::WidgetTransform;
    use crate::ui::{OffscreenRender, ViewRegistry};

    #[test]
    fn effect_measurement_keeps_source_placement_and_workspace_in_sync() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let owner = world.spawn_empty();
        world.insert(
            owner,
            DropShadow::new(source)
                .with_blur_radius(2)
                .with_offset(2, -1),
        );
        world.insert(owner, DropGlow::new(source).with_blur_radius(2));
        let owner_rect = Rect::new(600, 700, 1, 1);
        for pixels in [4, 8, 16] {
            let geometry = SourceGeometry {
                plane: PlaneLayout::packed(pixels, pixels, ColorFormat::RGBA8888).unwrap(),
                layout_rect: Rect::new(100, 50, 8, 8),
                local_bounds: Rect::new(-3, -2, 8, 8),
            };
            for (view, offset, expected) in [
                (
                    view(),
                    (Fixed::from_int(2), Fixed::from_int(-1)),
                    Rect::new(95, 43, 16, 16),
                ),
                (
                    glow_view(),
                    (Fixed::ZERO, Fixed::ZERO),
                    Rect::new(95, 46, 12, 12),
                ),
            ] {
                let measure = view.measurement().unwrap();
                let measured = measure(&world, owner, owner_rect, &|entity| {
                    (entity == source).then_some(geometry)
                })
                .unwrap();
                let (plane, bounds) =
                    effect_geometry(geometry, Fixed::from_int(2), offset).unwrap();
                assert_eq!(bounds, expected);
                assert_eq!(measured.bounds, Some(bounds));
                assert_eq!(measured.workspace, Some(plane.layout));
                assert!(measure(&world, owner, owner_rect, &|_| None).is_none());
            }
            for radius in [Fixed::ZERO, Fixed::MAX] {
                assert!(effect_geometry(geometry, radius, (Fixed::ZERO, Fixed::ZERO)).is_none());
            }
            for local_x in [0, 8] {
                assert!(
                    effect_geometry(
                        SourceGeometry {
                            layout_rect: Rect::new(Fixed::MAX, 50, 8, 8),
                            local_bounds: Rect::new(local_x, -2, 8, 8),
                            ..geometry
                        },
                        Fixed::from_int(2),
                        (Fixed::ZERO, Fixed::ZERO),
                    )
                    .is_none()
                );
            }
        }
    }

    #[test]
    fn effect_planes_preserve_logical_extents_at_each_pixel_density() {
        for pixels in [4, 8, 16] {
            let source = PlaneLayout::packed(pixels, pixels, ColorFormat::RGBA8888).unwrap();
            let plane = effect_plane(
                source,
                Rect::new(0, 0, 8, 8),
                Fixed::from_int(2),
                (Fixed::from_int(-2), Fixed::from_int(1)),
            )
            .unwrap();
            assert_eq!(plane.layout.width(), pixels * 2);
            assert_eq!(plane.layout.height(), pixels * 2);
            assert_eq!(plane.bounds, Rect::new(-4, -4, 16, 16));
            assert_eq!(plane.padding, (pixels / 2, pixels / 2));
            assert_eq!(
                plane.alpha.0,
                alpha_for_radius(Fixed::from_ratio(pixels.into(), 4))
            );
            assert_eq!(plane.alpha.0, plane.alpha.1);
        }

        let source = PlaneLayout::packed(4, 2, ColorFormat::RGBA8888).unwrap();
        let plane = effect_plane(
            source,
            Rect::new(0, 0, 8, 8),
            Fixed::from_int(2),
            (Fixed::from_int(-1), Fixed::ONE),
        )
        .unwrap();
        assert_eq!(plane.padding, (2, 1));
        assert_eq!(plane.bounds, Rect::new(-4, -4, 16, 16));
        assert_eq!(
            plane.alpha,
            (alpha_for_radius(Fixed::ONE), alpha_for_radius(Fixed::HALF))
        );
    }

    #[test]
    fn effect_planes_reject_unrepresentable_padding_and_empty_sources() {
        let source = PlaneLayout::packed(8, 8, ColorFormat::RGBA8888).unwrap();
        for (bounds, radius, offset) in [
            (
                Rect::new(0, 0, 8, 8),
                Fixed::MAX,
                (Fixed::ZERO, Fixed::ZERO),
            ),
            (Rect::new(0, 0, 8, 8), Fixed::ONE, (Fixed::MIN, Fixed::ZERO)),
            (
                Rect::new(0, 0, Fixed::MAX, 8),
                Fixed::ONE,
                (Fixed::ZERO, Fixed::ZERO),
            ),
            (
                Rect::new(0, 0, 0, 8),
                Fixed::ONE,
                (Fixed::ZERO, Fixed::ZERO),
            ),
        ] {
            assert!(effect_plane(source, bounds, radius, offset).is_none());
        }
        let empty = PlaneLayout::packed(0, 8, ColorFormat::RGBA8888).unwrap();
        assert!(
            effect_plane(
                empty,
                Rect::new(0, 0, 8, 8),
                Fixed::ONE,
                (Fixed::ZERO, Fixed::ZERO)
            )
            .is_none()
        );
    }

    #[test]
    fn effect_planes_accept_valid_shapes_with_wide_intermediate_values() {
        let source = PlaneLayout::packed(8192, 8192, ColorFormat::RGBA8888).unwrap();
        let plane = effect_plane(
            source,
            Rect::new(0, 0, 1_048_576, 1_048_576),
            Fixed::from_int(1_048_576),
            (Fixed::ZERO, Fixed::ZERO),
        )
        .unwrap();
        assert_eq!(plane.padding, (8192, 8192));
        assert_eq!(plane.layout.width(), 24576);
        assert_eq!(plane.layout.height(), 24576);
        assert_eq!(
            plane.bounds,
            Rect::new(-1_048_576, -1_048_576, 3_145_728, 3_145_728)
        );
    }

    #[crate::compose]
    fn effect_sources() {
        ui! {
            Row (width: 24, height: 8) {
                View (id: "shadow_source", width: 8, height: 8)
                View (id: "glow_source", width: 8, height: 8)
                View (id: "effects", width: 8, height: 8) [
                    DropShadow::new(id("shadow_source")),
                    DropGlow::new(id("glow_source")),
                    WidgetTransform(Transform::translate(Fixed::from_int(7), Fixed::from_int(11))),
                ]
            }
        };
    }

    #[test]
    fn effect_dependencies_keep_independent_sources_and_consumer_transform() {
        let mut app = App::headless(24, 8);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, effect_sources());
        let shadow_source = app.world.find_by_id("shadow_source").unwrap();
        let glow_source = app.world.find_by_id("glow_source").unwrap();
        let effects = app.world.find_by_id("effects").unwrap();
        let transform = app.world.get::<WidgetTransform>(effects).unwrap().0;

        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(crate::ui::offscreen::texture_requested(
            &app.world,
            shadow_source
        ));
        assert!(crate::ui::offscreen::texture_requested(
            &app.world,
            glow_source
        ));
        assert_eq!(
            app.world.get::<WidgetTransform>(effects).unwrap().0,
            transform
        );
        assert!(app.world.get::<OffscreenRender>(shadow_source).is_none());
        assert!(app.world.get::<OffscreenRender>(glow_source).is_none());

        app.world
            .insert(shadow_source, OffscreenRender::with_scale(Fixed::HALF));
        app.world.get_mut::<DropShadow>(effects).unwrap().source = glow_source;
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(!crate::ui::offscreen::texture_requested(
            &app.world,
            shadow_source
        ));
        assert!(crate::ui::offscreen::texture_requested(
            &app.world,
            glow_source
        ));
        assert_eq!(
            crate::ui::offscreen::config(&app.world, shadow_source)
                .unwrap()
                .scale,
            Fixed::HALF
        );

        app.world.remove::<DropShadow>(effects);
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(crate::ui::offscreen::texture_requested(
            &app.world,
            glow_source
        ));
        app.world.remove::<DropGlow>(effects);
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(!crate::ui::offscreen::texture_requested(
            &app.world,
            glow_source
        ));
        assert!(crate::ui::offscreen::config(&app.world, glow_source).is_none());
        assert_eq!(
            app.world.get::<WidgetTransform>(effects).unwrap().0,
            transform
        );
    }
}
