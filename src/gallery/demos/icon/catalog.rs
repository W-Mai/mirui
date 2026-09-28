use crate::ui::icons::IconAsset;
use crate::ui::icons::{
    ICON_ARROW_DOWN, ICON_ARROW_LEFT, ICON_ARROW_RIGHT, ICON_ARROW_UP, ICON_CHECK,
    ICON_CHEVRON_DOWN, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, ICON_CHEVRON_UP, ICON_CIRCLE,
    ICON_CROSS, ICON_HEART, ICON_HOME, ICON_MINUS, ICON_PAUSE, ICON_PLAY, ICON_PLUS, ICON_SQUARE,
    ICON_STAR, ICON_STOP,
};
use crate::ui::theme::ColorToken;

pub(super) fn icons() -> [(IconAsset, ColorToken); 20] {
    [
        (ICON_HOME, ColorToken::Primary),
        (ICON_CHECK, ColorToken::Success),
        (ICON_CROSS, ColorToken::Error),
        (ICON_PLUS, ColorToken::OnSurface),
        (ICON_MINUS, ColorToken::OnSurface),
        (ICON_ARROW_LEFT, ColorToken::OnSurfaceVariant),
        (ICON_ARROW_RIGHT, ColorToken::OnSurfaceVariant),
        (ICON_ARROW_UP, ColorToken::OnSurfaceVariant),
        (ICON_ARROW_DOWN, ColorToken::OnSurfaceVariant),
        (ICON_CHEVRON_LEFT, ColorToken::Primary),
        (ICON_CHEVRON_RIGHT, ColorToken::Primary),
        (ICON_CHEVRON_UP, ColorToken::Primary),
        (ICON_CHEVRON_DOWN, ColorToken::Primary),
        (ICON_STAR, ColorToken::Primary),
        (ICON_HEART, ColorToken::Error),
        (ICON_PLAY, ColorToken::Success),
        (ICON_PAUSE, ColorToken::OnSurface),
        (ICON_STOP, ColorToken::Error),
        (ICON_CIRCLE, ColorToken::OnSurfaceVariant),
        (ICON_SQUARE, ColorToken::OnSurfaceVariant),
    ]
}
