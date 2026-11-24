use mirui::prelude::{Color, ColorToken, Theme};

/// Paper, ink and the blue selection accent used by the manual.
pub(crate) fn book(dark: bool) -> Theme {
    let rgb = Color::rgb;
    let (base, paper, ink, inset, muted, line, blue) = if dark {
        (
            Theme::dark(),
            rgb(20, 21, 24),
            rgb(236, 236, 238),
            rgb(32, 34, 39),
            rgb(159, 162, 172),
            rgb(67, 70, 80),
            rgb(111, 130, 255),
        )
    } else {
        (
            Theme::light(),
            rgb(255, 255, 255),
            rgb(30, 32, 36),
            rgb(242, 243, 246),
            rgb(95, 101, 113),
            rgb(212, 215, 221),
            rgb(52, 82, 255),
        )
    };
    base.with_many([
        (ColorToken::Surface, paper),
        (ColorToken::OnSurface, ink),
        (ColorToken::SurfaceVariant, inset),
        (ColorToken::OnSurfaceVariant, muted),
        (ColorToken::Outline, line),
        (ColorToken::Primary, blue),
        (
            ColorToken::Secondary,
            if dark { rgb(43, 45, 51) } else { ink },
        ),
        (ColorToken::OnSecondary, rgb(247, 247, 248)),
        (
            ColorToken::OnPrimary,
            if dark { rgb(14, 15, 17) } else { paper },
        ),
    ])
}
