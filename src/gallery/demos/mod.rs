pub mod animation;
pub mod blur_filter;
pub mod book_flip;
pub mod builder_form;
pub mod butterfly;
pub mod clip_path;
pub mod composite;
pub mod cover_flow;
pub mod curve_text;
pub mod curve_text_compact;
pub mod custom_view;
pub mod effect_glass;
pub mod effect_panels;
pub mod fill_rules;
pub mod flip_card;
pub mod gradient;
pub mod i18n;
pub mod icon;
pub mod image_flip;
pub mod interaction_lab;
pub mod kinetic_console;
mod lab_scroll;
pub mod layout_lab;
pub mod lazy_list;
pub mod life;
pub mod life_compact;
mod motion;
pub mod nested_scroll;
pub mod niche;
pub mod offscreen;
pub mod offscreen_modal;
pub mod orbit_console;
pub mod particles;
pub mod persistence_counter;
pub mod pinch_rotate;
pub mod render_showcase;
pub mod scroll;
pub mod shapes;
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
pub mod transform;
pub mod typography_lab;
pub mod vector_mandala;
pub mod widgets;
pub mod widgets_compact;

pub(super) const PROJECTIVE_SPIN_PHASE: crate::types::Fixed = crate::types::Fixed::from_ratio(1, 4);

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
