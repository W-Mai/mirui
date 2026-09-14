use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::Renderer;
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::render::texture::{ColorFormat, TexBuf, Texture};
use crate::types::{Fixed, Point, Rect};
use crate::ui::offscreen::{
    OffscreenAlphaMode, OffscreenAutoAdded, OffscreenRender, WidgetTextureAccess, WidgetTextureRef,
};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};

pub struct DropShadow {
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

fn drop_shadow_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(shadow) = world.get::<DropShadow>(entity) else {
        return;
    };
    render_drop_effect(
        renderer,
        world,
        shadow.source,
        shadow.blur_radius,
        shadow.offset,
        shadow.color,
        shadow.opacity,
        CompositeMode::SourceOver,
        rect,
        ctx,
    );
}

pub fn view() -> View {
    View::new("DropShadow", 60, drop_shadow_render)
        .with_filter::<DropShadow>()
        .with_attach(drop_effect_attach::<DropShadow>)
}

pub fn drop_effect_attach<T: 'static>(world: &mut World, entity: Entity) {
    let source = {
        if let Some(eff) = world.get::<DropShadow>(entity) {
            eff.source
        } else if let Some(eff) = world.get::<DropGlow>(entity) {
            eff.source
        } else {
            return;
        }
    };
    if world.get::<WidgetTextureRef>(entity).is_none() {
        world.insert(entity, WidgetTextureRef(source));
    }
    if world.get::<OffscreenRender>(source).is_none() {
        world.insert(source, OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }
    if world.get::<OffscreenAlphaMode>(source).is_none() {
        world.insert(source, OffscreenAlphaMode::clear_transparent());
    }
}

#[allow(clippy::too_many_arguments)]
fn render_drop_effect(
    renderer: &mut dyn Renderer,
    world: &World,
    source: Entity,
    blur_radius: Fixed,
    offset: (Fixed, Fixed),
    color: ThemedColor,
    opacity: u8,
    composite: CompositeMode,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if blur_radius <= Fixed::ZERO {
        return;
    }
    let Some(snap) = world.texture_of(source) else {
        return;
    };
    let src = snap.borrow();

    let theme = ctx.theme(world);
    let resolved = color.resolve(theme);
    let r = resolved.r;
    let g = resolved.g;
    let b = resolved.b;

    let padding = blur_radius.ceil() + offset.0.abs().max(offset.1.abs()).ceil();
    let pad_i = padding.to_int().max(0) as u16;

    let src_w = src.width;
    let src_h = src.height;
    let dst_w = src_w + 2 * pad_i;
    let dst_h = src_h + 2 * pad_i;
    if dst_w == 0 || dst_h == 0 {
        return;
    }

    let mut work = Texture::owned(dst_w, dst_h, ColorFormat::RGBA8888);
    if let TexBuf::Owned(ref mut buf) = work.buf {
        for b in buf.iter_mut() {
            *b = 0;
        }
        let dst_stride = dst_w as usize * 4;
        let src_stride = src.stride;
        match src.format {
            ColorFormat::RGBA8888 => {
                for y in 0..src_h {
                    let si = (y as usize) * src_stride;
                    let di = ((y + pad_i) as usize) * dst_stride + (pad_i as usize) * 4;
                    let copy_len = (src_w as usize) * 4;
                    buf[di..di + copy_len].copy_from_slice(&src.buf.as_slice()[si..si + copy_len]);
                }
            }
            _ => return,
        }
    }

    let alpha = alpha_for_radius(blur_radius);
    {
        let r = crate::types::Rect::new(
            crate::types::Fixed::ZERO,
            crate::types::Fixed::ZERO,
            crate::types::Fixed::from_int(work.width as i32),
            crate::types::Fixed::from_int(work.height as i32),
        );
        iir_blur_inplace(&mut work, alpha, r);
    }

    if let TexBuf::Owned(ref mut buf) = work.buf {
        let stride = dst_w as usize * 4;
        for y in 0..dst_h {
            for x in 0..dst_w {
                let i = (y as usize) * stride + (x as usize) * 4;
                let a = buf[i + 3] as u32;
                let tinted_a = (a * opacity as u32 / 255).min(255) as u8;
                buf[i] = r;
                buf[i + 1] = g;
                buf[i + 2] = b;
                buf[i + 3] = tinted_a;
            }
        }
    }

    let src_rect = world
        .get::<crate::ui::ComputedRect>(source)
        .map(|c| c.0)
        .unwrap_or_else(|| ctx.transform.apply_rect_bbox(*rect));

    let blit_x = src_rect.x + offset.0 - Fixed::from_int(pad_i as i32);
    let blit_y = src_rect.y + offset.1 - Fixed::from_int(pad_i as i32);

    ctx.draw(
        renderer,
        &DrawCommand::Blit {
            pos: Point::new(blit_x, blit_y),
            size: Point::new(Fixed::from_int(dst_w as i32), Fixed::from_int(dst_h as i32)),
            transform: ctx.transform,
            quad: None,
            texture: &work,
            opa: 255,
            radius: Fixed::ZERO,
            composite,
        },
        ctx.clip,
    );
}

pub struct DropGlow {
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

fn drop_glow_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(glow) = world.get::<DropGlow>(entity) else {
        return;
    };
    render_drop_effect(
        renderer,
        world,
        glow.source,
        glow.blur_radius,
        (Fixed::ZERO, Fixed::ZERO),
        glow.color,
        glow.opacity,
        CompositeMode::Screen,
        rect,
        ctx,
    );
}

pub fn glow_view() -> View {
    View::new("DropGlow", 60, drop_glow_render)
        .with_filter::<DropGlow>()
        .with_attach(drop_effect_attach::<DropGlow>)
}
