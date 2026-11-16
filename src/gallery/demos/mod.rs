pub mod animation;
pub mod atlas_restoration;
pub mod blur_filter;
pub mod book_flip;
pub mod builder_form;
pub mod butterfly;
pub mod chord_prompter;
pub mod clip_path;
#[cfg(feature = "std")]
mod clock;
mod compact_layout;
pub mod composite;
pub mod cover_flow;
pub mod curve_text;
pub mod curve_text_compact;
pub mod custom_view;
pub mod echo_walker;
pub mod editor_lab;
pub mod effect_glass;
pub mod effect_panels;
pub mod fill_rules;
pub mod flip_card;
pub mod folding_ark;
pub mod glass_light_toggle;
pub mod gradient;
pub mod i18n;
pub mod icon;
pub mod image_flip;
#[cfg(feature = "std")]
mod instruments;
pub mod interaction_lab;
pub mod iris;
pub mod kinetic_console;
pub mod layout_lab;
pub mod lazy_list;
pub mod life;
pub mod life_compact;
pub mod logic_circuit;
pub mod lumen_lab;
pub mod marble_play;
pub mod module_factory;
pub mod moss_study;
mod motion;
pub mod nearby;
pub mod nested_scroll;
pub mod niche;
pub mod offscreen;
pub mod offscreen_modal;
pub mod orbit_console;
pub mod orbital_mission;
pub mod page_studio;
pub mod particles;
pub mod persistence_counter;
pub mod persistence_settings;
pub mod pinch_rotate;
#[cfg(feature = "std")]
pub mod pixel_flight;
pub mod pixel_loom;
pub mod pocket_post;
pub mod render_showcase;
pub mod scroll;
pub mod shapes;
#[cfg(feature = "std")]
pub mod signal_scope;
pub mod slider_value_changed;
pub mod spatial_anim;
pub mod state_computed;
pub mod state_counter;
pub mod state_effect;
pub mod state_form;
pub mod state_keyed;
pub mod state_list;
pub mod state_show;
pub mod state_todo;
pub mod stroke_styles;
pub mod subpixel;
pub mod tabbar;
pub mod text_input;
pub mod theme_swap;
pub mod three_body;
pub mod tidal_atlas;
pub mod transform;
pub mod twin_beacons;
pub mod typography_lab;
pub mod vector_mandala;
pub mod widgets;
pub mod widgets_compact;

pub(super) const PROJECTIVE_SPIN_PHASE: crate::types::Fixed = crate::types::Fixed::from_ratio(1, 4);

/// Where a visible text sits and how much room its laid out lines take.
#[derive(Clone, Debug)]
pub struct TextFit {
    pub entity: crate::ecs::Entity,
    pub text: alloc::string::String,
    /// The box the layout gave the text.
    pub rect: crate::types::Rect,
    /// The laid out lines, placed in the box by the paragraph alignment. It
    /// can reach outside `rect` when the text does not fit.
    pub lines: crate::types::Rect,
}

impl TextFit {
    /// Whether the lines are wider than the box, or taller by more than
    /// `slack` pixels (a line box is taller than its glyphs by the descent).
    pub fn overflows(&self, slack: i32) -> bool {
        self.lines.w > self.rect.w
            || self.lines.h > self.rect.h + crate::types::Fixed::from_int(slack)
    }
}

/// Measures every visible text of `world`, for the Gallery audit tools.
pub fn text_fits(world: &crate::ecs::World) -> alloc::vec::Vec<TextFit> {
    use alloc::vec::Vec;

    use crate::text::{TextGeometry, TextLayoutHandle};
    use crate::types::Fixed;
    use crate::types::fixed::from_textflow;
    use crate::ui::ComputedRect;
    use crate::ui::outputs::Outputs;
    use crate::ui::widgets::{Text, TextAlign, TextVerticalAlign};

    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Vec::new();
    };
    let storage = outputs.storage();
    let entities: Vec<_> = world.query::<Text>().collect();
    let mut fits = Vec::new();
    for entity in entities {
        if crate::ui::branch::is_hidden_in_tree(world, entity)
            || world.has::<crate::text::TextPath>(entity)
        {
            continue;
        }
        let (Some(rect), Some(handle)) = (
            world.get::<ComputedRect>(entity).map(|value| value.0),
            world.get::<TextLayoutHandle>(entity).copied(),
        ) else {
            continue;
        };
        let Some(layout) = storage.layouts.get(handle) else {
            continue;
        };
        let text = world.get::<Text>(entity).expect("text entity");
        let w = from_textflow(layout.measure().width);
        let h = from_textflow(layout.measure().height);
        let half = |room: Fixed| room / Fixed::from_int(2);
        let x = match text.paragraph().align {
            TextAlign::Center => rect.x + half(rect.w - w),
            TextAlign::End => rect.x + rect.w - w,
            TextAlign::Start | TextAlign::Justify => rect.x,
        };
        let y = match text.paragraph().vertical_align {
            TextVerticalAlign::Center => rect.y + half(rect.h - h),
            TextVerticalAlign::End => rect.y + rect.h - h,
            TextVerticalAlign::Start => rect.y,
        };
        fits.push(TextFit {
            entity,
            text: text.resolve(world.resource()).as_ref().into(),
            rect,
            lines: crate::types::Rect { x, y, w, h },
        });
    }
    fits
}

#[cfg(test)]
pub(super) fn assert_text_layouts_fit(world: &crate::ecs::World) {
    use alloc::vec::Vec;

    use crate::text::{TextGeometry, TextLayoutHandle};
    use crate::types::fixed::from_textflow;
    use crate::ui::ComputedRect;
    use crate::ui::outputs::Outputs;
    use crate::ui::widgets::{Text, TextOverflow, TextWrap};

    let entities: Vec<_> = world.query::<Text>().collect();
    let storage = world
        .resource::<Outputs<TextGeometry>>()
        .expect("text geometry outputs")
        .storage();
    for entity in entities {
        if crate::ui::branch::is_hidden_in_tree(world, entity) {
            continue;
        }
        if world.has::<crate::text::TextPath>(entity) {
            continue;
        }
        let Some(rect) = world.get::<ComputedRect>(entity).map(|value| value.0) else {
            continue;
        };
        let Some(handle) = world.get::<TextLayoutHandle>(entity).copied() else {
            continue;
        };
        let text = world.get::<Text>(entity).expect("text entity");
        let layout = storage.layouts.get(handle).expect("live text layout");
        let paragraph = text.paragraph();
        if let Some(max_lines) = paragraph.max_lines {
            assert!(
                layout.lines().len() <= usize::from(max_lines),
                "{:?} exceeds {max_lines} lines: {}",
                entity,
                text.resolve(world.resource())
            );
        }
        if paragraph.wrap != TextWrap::NoWrap || paragraph.overflow == TextOverflow::Ellipsis {
            assert!(
                from_textflow(layout.measure().width) <= rect.w,
                "{:?} exceeds its width {:?}: {}",
                entity,
                rect.w,
                text.resolve(world.resource())
            );
        }
        assert!(
            from_textflow(layout.measure().height) <= rect.h,
            "{:?} exceeds its height {:?}: {}",
            entity,
            rect.h,
            text.resolve(world.resource())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projective_spin_phase_avoids_singular_edge_on_frames() {
        use crate::types::Fixed;
        use crate::types::Transform3D;

        for half_step in 0..720 {
            let angle = PROJECTIVE_SPIN_PHASE + Fixed::from_ratio(half_step, 2);
            assert!(!Fixed::cos_deg(angle).is_zero(), "angle {angle:?}");
            assert!(!Fixed::cos_deg(-angle).is_zero(), "angle -{angle:?}");
            assert!(
                Transform3D::rotate_y_perspective(angle, Fixed::from_int(400))
                    .inverse()
                    .is_some(),
                "angle {angle:?}"
            );
        }
    }
}
