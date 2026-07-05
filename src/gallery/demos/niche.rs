use crate::ecs::Entity;
use crate::mold;
use crate::prelude::*;
use crate::ui::widgets::Text;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

//~focus-start
mold!(Card {
    Column (direction: FlexDirection::Column, grow: 1.0) {
        View (
            bg_color: ColorToken::Primary,
            padding: Padding::all(8),
            height: 32
        ) {
            @@header {
                Text ("(untitled)", text_color: ColorToken::OnPrimary)
            }
        }
        View (
            bg_color: ColorToken::SurfaceVariant,
            padding: Padding::all(12),
            direction: FlexDirection::Column,
            grow: 1.0
        ) {
            @@body {
                Text (
                    "no body yet",
                    text_color: ColorToken::OnSurfaceVariant
                )
            }
        }
        View (
            bg_color: ColorToken::Surface,
            padding: Padding::all(8),
            direction: FlexDirection::Row,
            height: 32
        ) {
            @@footer
        }
    }
});
//~focus-end

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(24),
            direction: FlexDirection::Column
        ) {
            Card (grow: 1.0) {
                @header {
                    Text ("Alert", text_color: ColorToken::OnPrimary)
                }
                @body {
                    Column (direction: FlexDirection::Column, grow: 1.0) {
                        Text (
                            "@header @body @footer",
                            text_color: ColorToken::OnSurface
                        )
                        Text (
                            "each pick their own slot.",
                            text_color: ColorToken::OnSurface
                        )
                    }
                }
                @footer {
                    Text ("dismiss", text_color: ColorToken::OnSurfaceVariant)
                }
            }
            View (height: 16)
            Card (height: 120) {
                @body {
                    Column (direction: FlexDirection::Column, grow: 1.0) {
                        Text (
                            "body only. header uses",
                            text_color: ColorToken::OnSurface
                        )
                        Text (
                            "its @@ fallback content.",
                            text_color: ColorToken::OnSurface
                        )
                    }
                }
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(mold!(Card));
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiScope;
    use crate::ui::{IdMap, NicheMap, Parent, ViewRegistry};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::default();
        reg.insert(mold!(Card));
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();

        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);

        let cards: Vec<Entity> = world.query::<Card>().collect();
        assert_eq!(cards.len(), 2, "expected two Card widgets");

        let first = cards[0];
        let map = world
            .get::<NicheMap>(first)
            .expect("card_attach registers NicheMap");
        let body = map.get("body").expect("body niche registered");

        let texts: Vec<Entity> = world.query::<crate::ui::widgets::Text>().collect();
        let body_populated = texts.into_iter().any(|t| {
            let mut cur = Some(t);
            while let Some(e) = cur {
                if e == body {
                    return true;
                }
                cur = world.get::<Parent>(e).map(|p| p.0);
            }
            false
        });
        assert!(
            body_populated,
            "@body slot subtree should contain at least one Text",
        );
    }
}
