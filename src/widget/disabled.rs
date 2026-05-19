/// Disables interaction on the entity and its descendants. Layout
/// and render still run; gestures, focus, and key dispatch all
/// skip; colours route through [`crate::widget::theme::WidgetState::Disabled`].
pub struct Disabled;
