use crate::ecs::{Component, Entity, World};
use crate::prelude::*;
use crate::ui::widgets::Text;
use crate::ui::{NicheMap, Parent, view::View};

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

/// Container widget with three named slots.
///
/// A `Card` reserves `header` / `body` / `footer` [niches](NicheMap) up
/// front. Call sites populate any subset with `@name { ... }` blocks
/// inside the `ui!` macro; unused slots stay in the tree but empty.
#[derive(Default)]
pub struct Card;

impl Component for Card {}

fn card_attach(world: &mut World, entity: Entity) {
    if world.get::<NicheMap>(entity).is_some() {
        return;
    }
    let header = spawn_slot(world, entity);
    let body = spawn_slot(world, entity);
    let footer = spawn_slot(world, entity);
    world.insert(
        entity,
        NicheMap::from([("header", header), ("body", body), ("footer", footer)]),
    );
}

fn spawn_slot(world: &mut World, parent: Entity) -> Entity {
    let e = world.spawn_empty();
    world.insert(e, Parent(parent));
    e
}

fn card_render(
    _: &mut dyn crate::render::renderer::Renderer,
    _: &World,
    _: Entity,
    _: &crate::types::Rect,
    _: &mut crate::ui::view::ViewCtx,
) {
    // Card is a pure container; Style drives the bg / border fill via
    // the generic style View. Kept as an empty callback so the widget
    // still registers in the ViewRegistry.
}

pub fn view() -> View {
    View::new("Card", 60, card_render)
        .with_filter::<Card>()
        .with_attach(card_attach)
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(24), direction: FlexDirection::Column) {
            Card () {
                @header {
                    Text (
                        "Alert",
                        text_color: ColorToken::OnPrimary
                    )
                }
                @body {
                    Text (
                        "@header / @body / @footer are named niches on the Card widget.",
                        text_color: ColorToken::OnSurface
                    )
                    View (height: 6)
                    Text (
                        "Only the slots you fill get children; the rest stay empty and pad the layout.",
                        text_color: ColorToken::OnSurfaceVariant
                    )
                }
                @footer {
                    View (
                        bg_color: ColorToken::Primary,
                        border_radius: 6,
                        padding: Padding::all(6)
                    ) {
                        Text ("Dismiss", text_color: ColorToken::OnPrimary)
                    }
                }
            }
            View (height: 16)
            Card () {
                @body {
                    Text (
                        "Body-only Card — the header and footer niches exist but are empty.",
                        text_color: ColorToken::OnSurface
                    )
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(view());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::default();
        reg.insert(view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();

        build_widgets(&mut world, parent);

        let cards: Vec<Entity> = world.query::<Card>().collect();
        assert_eq!(cards.len(), 2, "expected two Card widgets");

        let first = cards[0];
        let map = world
            .get::<NicheMap>(first)
            .expect("card_attach registers NicheMap");
        let body = map.get("body").expect("body niche registered");

        // ui! wires slot children with a Parent component (Children is
        // only maintained by WidgetBuilder-tracked spawns). Walk the
        // Text query to confirm the @body slot picked something up.
        let texts: Vec<Entity> = world.query::<crate::ui::widgets::Text>().collect();
        let body_populated = texts
            .into_iter()
            .any(|t| world.get::<Parent>(t).is_some_and(|p| p.0 == body));
        assert!(
            body_populated,
            "@body slot should have received Text children via Parent",
        );
    }
}
