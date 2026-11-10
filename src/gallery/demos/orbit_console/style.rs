use crate::prelude::ColorToken;

pub(super) const BG: ColorToken = ColorToken::Surface;
pub(super) const SURFACE: ColorToken = ColorToken::SurfaceVariant;
pub(super) const BORDER: ColorToken = ColorToken::Outline;
/// Selected segment of a segmented control.
pub(super) const RAISED: ColorToken = ColorToken::Outline;
pub(super) const TEXT: ColorToken = ColorToken::OnSurface;
pub(super) const TEXT_MUTED: ColorToken = ColorToken::OnSurfaceVariant;
/// The single accent: the focused orbit, live data and the intensity fill.
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
/// Connection and live status.
pub(super) const OK: ColorToken = ColorToken::Success;
