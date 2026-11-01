use crate::prelude::*;
use crate::ui::theme::{Theme, ThemeInfo};
pub(crate) mod actor {
    use crate::prelude::ColorToken;

    pub const HOST: ColorToken = ColorToken::Custom("actor-host");
    pub const HOST_LIGHT: ColorToken = ColorToken::Custom("actor-host-light");
    pub const GUEST: ColorToken = ColorToken::Custom("actor-guest");
    pub const GUEST_LIGHT: ColorToken = ColorToken::Custom("actor-guest-light");
    pub const SKIN: ColorToken = ColorToken::Custom("actor-skin");
    pub const BOOTS: ColorToken = ColorToken::Custom("actor-boots");
    pub const PACK: ColorToken = ColorToken::Custom("actor-pack");

    // Player identity colors stay consistent when the campsite palette changes.
    pub const COLORS: [(ColorToken, u32); 7] = [
        (HOST, 0xdb603d),
        (HOST_LIGHT, 0xffad68),
        (GUEST, 0x6477cf),
        (GUEST_LIGHT, 0xb5c7ff),
        (SKIN, 0xffe0aa),
        (BOOTS, 0x302b38),
        (PACK, 0xf4c85f),
    ];
}

pub(super) fn theme(index: usize) -> Theme {
    use ColorToken::*;
    let values = match index % 3 {
        0 => [
            0xf2eddb, 0x293a31, 0x42664c, 0xe6e7d3, 0xbdc6a9, 0x63705b, 0xc68c3d, 0xad573b,
            0xd6dfc7, 0xaabda0, 0x243f33, 0xf2eddb,
        ],
        1 => [
            0xf5e9d4, 0x482f29, 0xa74e37, 0xecddc4, 0xcebaa0, 0x82614f, 0xc18a32, 0x70835c,
            0xe8cdb0, 0xc49e81, 0x623b30, 0xf5e9d4,
        ],
        _ => [
            0x283933, 0xf2e8c9, 0xd2aa65, 0x344b40, 0x58705b, 0xb5c6a4, 0xdcaa56, 0xde8760,
            0x3c5547, 0x577058, 0x1c2e26, 0x1c2e26,
        ],
    };
    let tokens = [
        Surface,
        OnSurface,
        Primary,
        SurfaceVariant,
        Outline,
        OnSurfaceVariant,
        Custom("gold"),
        Custom("clay"),
        Custom("sky"),
        Custom("hill"),
        Custom("deep"),
        OnPrimary,
    ];
    let mut theme = Theme::light().with_info(ThemeInfo::new(
        ["forest", "sunset", "night"][index % 3],
        ["森林", "落日", "夜色"][index % 3],
        "",
    ));
    for (token, rgb) in tokens.into_iter().zip(values).chain(actor::COLORS) {
        theme.set(
            token,
            Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8),
        );
    }
    theme.set(Custom("header-text"), Color::rgb(243, 236, 216));
    theme.set(Custom("header-detail"), Color::rgb(199, 208, 181));
    theme
}
