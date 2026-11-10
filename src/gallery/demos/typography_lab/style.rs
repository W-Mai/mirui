use crate::prelude::ColorToken;

pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const PANEL_ALT: ColorToken = ColorToken::Surface;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
/// Pressed option buttons.
pub(super) const RAISED: ColorToken = ColorToken::Outline;
/// Interactive fills, left-to-right carets and full coverage.
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
/// Guide strokes such as the text path.
pub(super) const SLATE: ColorToken = ColorToken::Secondary;
/// The caret under the path probe.
pub(super) const HIT: ColorToken = ColorToken::Success;
/// Right-to-left carets.
pub(super) const VIOLET: ColorToken = ColorToken::Tertiary;

pub(super) const ACTIVE_RENDER_PATH: &str = "Glyph run · A8/SDF representations";
