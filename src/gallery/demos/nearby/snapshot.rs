use crate::prelude::*;
use crate::render::command::{CompositeMode, DrawCommand};
use crate::ui::{OffscreenRender, Outputs, WidgetTexture};

#[component]
#[retain_previous(WidgetTexture)]
pub(super) struct BackdropSnapshot {
    #[depends_on(WidgetTexture)]
    pub source: Entity,
}

#[view(component = BackdropSnapshot, priority = 60)]
pub(super) fn backdrop_snapshot(
    snapshot: component!(),
    entity: entity!(),
    outputs: res!(Outputs<WidgetTexture>),
    paint: paint!(),
) {
    let rect = paint.rect();
    let previous = outputs.previous(entity);
    let previous =
        previous.filter(|image| image.layout_rect().w == rect.w && image.layout_rect().h == rect.h);
    let current = previous.or_else(|| outputs.get(snapshot.source));
    let Some(image) = current else {
        return;
    };
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: paint.quad(),
        texture: image.texture(),
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    });
}

#[compose]
pub(super) fn frozen_backdrop(source: Entity, state: model!(super::model::DisplayState)) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            visible: ${
                state.return_to_launcher() || state.launcher().deck().panel() > Fixed::ZERO
                    || state.launcher().menu()
            }
        ) [
            BackdropSnapshot { source },
            OffscreenRender::default(),
            crate::ui::IgnoreHitTest,
        ]
    }
}
