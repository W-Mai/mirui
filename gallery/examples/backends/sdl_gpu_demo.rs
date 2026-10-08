//! SDL GPU rendering with static panels, an image, and a draggable label.
//! The label uses Model-bound position properties and typed drag callbacks.

use mirui::app::plugins::{CacheReportPlugin, FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl_gpu::{SdlGpuFactory, SdlGpuSurface};
use mirui::ui::property::prop;
use mirui::ui::widgets::assets::*;
use mirui::ui::widgets::{Image, Text};

#[model]
struct DragPosition {
    #[observe]
    left: Fixed,
    #[observe]
    top: Fixed,
    offset: (Fixed, Fixed),
}

#[model]
impl DragPosition {
    fn begin_drag(&mut self, x: Fixed, y: Fixed) {
        self.offset = (x - self.left, y - self.top);
    }

    fn drag_to(&mut self, x: Fixed, y: Fixed) {
        self.left = x - self.offset.0;
        self.top = y - self.offset.1;
    }
}

#[event]
fn begin_drag(input: event!(DragStart), position: model!(DragPosition)) {
    position.begin_drag(input.x, input.y);
}

#[event]
fn move_drag(input: event!(DragMove), position: model!(DragPosition)) {
    position.drag_to(input.x, input.y);
}

fn sized(width: i32, height: i32) -> Style {
    Style {
        layout: LayoutStyle {
            width: Dimension::px(width),
            height: Dimension::px(height),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() {
    let backend = SdlGpuSurface::new("mirui SDL GPU — drag me", 640, 480);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());
    app.with_default_widgets()
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default())
        .add_plugin(mirui::app::plugins::ImageResourcesPlugin::default())
        .add_plugin(CacheReportPlugin::new(120));
    let root = app
        .spawn_root()
        .bg_color(Color::rgba(30, 30, 46, 255))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            grow: Fixed::ONE,
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();
    let position = app.add_model(DragPosition {
        left: Fixed::from_int(240),
        top: Fixed::from_int(30),
        offset: (Fixed::ZERO, Fixed::ZERO),
    });

    app.compose(root, |tree| {
        tree.spawn(Style {
            bg_color: Some(Color::rgba(32, 160, 240, 255).into()),
            border_color: Some(Color::rgba(240, 240, 255, 255).into()),
            border_width: Fixed::ONE,
            ..sized(200, 80)
        });
        tree.spawn(Style {
            bg_color: Some(Color::rgba(80, 240, 160, 128).into()),
            ..sized(200, 80)
        });
        tree.spawn(Text::build("SDL GPU BACKEND").style(Style {
            text_color: Color::rgba(255, 255, 255, 255).into(),
            ..sized(200, 20)
        }));
        tree.spawn(Image::build("thumbs_up").style(sized(
            IMG_THUMBS_UP.width as i32,
            IMG_THUMBS_UP.height as i32,
        )));

        let horizontal = position.clone();
        let vertical = position.clone();
        tree.spawn(
            Text::build("DRAG ME")
                .style(Style {
                    bg_color: Some(Color::rgba(240, 120, 60, 230).into()),
                    border_color: Some(Color::rgba(255, 255, 255, 255).into()),
                    border_width: Fixed::from_int(2),
                    border_radius: Fixed::from_int(12),
                    text_color: Color::rgba(255, 255, 255, 255).into(),
                    layout: LayoutStyle {
                        position: Position::Absolute,
                        width: Dimension::px(160),
                        height: Dimension::px(60),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .bind_property::<prop::Left>(move || Dimension::Px(horizontal.left()))
                .bind_property::<prop::Top>(move || Dimension::Px(vertical.top()))
                .on_event(DragStart::EVENT, begin_drag::callback(position.clone()))
                .on_event(DragMove::EVENT, move_drag::callback(position)),
        );
    });

    app.run();
}
