//! What the 128×128 display shows a musician who has no touch screen: the
//! colour roles, the status row, the count-in and the end of the song.

use super::palette::{Palette, contrast};
use super::tests::packet;
use super::*;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::Theme;

/// A headless 128×128 prompter and the session that drives it.
macro_rules! bench {
    ($app:ident, $session:ident) => {
        let mut $app = App::headless(128, 128);
        $app.with_default_widgets().with_default_systems();
        $app.add_resource(MonoClock::new(|| 0));
        let root = $app.spawn_root().id();
        let $session = install(&mut $app, root);
    };
}

fn rgb(color: Color) -> [u8; 3] {
    [color.r, color.g, color.b]
}

fn pixel<B: FramebufferAccess>(backend: &mut B, x: i32, y: i32) -> [u8; 3] {
    let texture = backend.framebuffer();
    let pixels = texture.read_pixels().unwrap();
    let at = y as usize * pixels.stride() + x as usize * 4;
    [
        pixels.bytes()[at],
        pixels.bytes()[at + 1],
        pixels.bytes()[at + 2],
    ]
}

/// Where the status row is in the frame: left, top and width.
fn header_at<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> (i32, i32, i32) {
    let header = app.world.find_by_id("prompter_header").unwrap();
    let rect = app.world.get::<crate::ui::ComputedRect>(header).unwrap().0;
    (rect.x.to_int(), rect.y.to_int(), rect.w.to_int())
}

#[test]
fn contrast_is_the_wcag_ratio() {
    let (black, white) = (Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
    assert_eq!(contrast(black, black), 1_000);
    assert!(contrast(black, white).abs_diff(21_000) < 60);
    // #777777 on white is the textbook 4.48:1.
    assert!(contrast(Color::rgb(119, 119, 119), white).abs_diff(4_480) < 80);
}

#[test]
fn every_role_keeps_its_contrast_on_dark_light_and_dim_themes() {
    use crate::gallery::showcase_theme;
    // Greys that sit close to their background, as a custom theme might.
    let dim = Theme::dark().with_many([
        (ColorToken::Surface, Color::rgb(40, 40, 44)),
        (ColorToken::OnSurface, Color::rgb(90, 90, 96)),
        (ColorToken::OnSurfaceVariant, Color::rgb(70, 70, 76)),
        (ColorToken::Primary, Color::rgb(70, 70, 120)),
        (ColorToken::Success, Color::rgb(50, 80, 50)),
        (ColorToken::Tertiary, Color::rgb(80, 60, 100)),
    ]);
    let themes = [
        Theme::dark(),
        Theme::light(),
        showcase_theme::dark(),
        showcase_theme::light(),
        dim,
    ];
    for theme in themes {
        let palette = Palette::of(&theme);
        for (role, color, least) in [
            ("ink", palette.ink, 7_000),
            ("past", palette.past, 3_200),
            ("now", palette.now, 4_500),
            ("chrome", palette.chrome, 4_500),
            ("needle", palette.needle, 3_000),
            ("ok", palette.ok, 4_500),
            ("queued", palette.queued, 4_500),
        ] {
            let ratio = contrast(color, palette.bg);
            assert!(
                ratio >= least,
                "{} {role}: {ratio} is under {least}",
                theme.id().as_str()
            );
        }
        // What was played recedes, but stays readable.
        assert!(contrast(palette.past, palette.bg) < contrast(palette.ink, palette.bg));
    }
}

#[test]
fn a_delayed_start_counts_every_beat_of_the_bar_and_ends_on_the_downbeat() {
    let mut app = App::headless(128, 128);
    let session = app
        .add_model(Session::new(
            Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap(),
        ))
        .finish();
    let mut link = DeviceLink::new(7);
    // Four beats of 625 ms.
    assert_eq!(link.write(&session, packet(1, 0, 7, 2500), 0)[0], 0);
    let mut counted = alloc::vec::Vec::new();
    for now in [
        0, 624_999, 625_000, 1_249_999, 1_250_000, 1_875_000, 2_499_999, 2_500_000,
    ] {
        session.advance(now);
        counted.push((session.count_in(), session.clock_active()));
    }
    assert_eq!(
        counted,
        [
            (1, false),
            (1, false),
            (2, false),
            (2, false),
            (3, false),
            (4, false),
            (4, false),
            (0, true)
        ]
    );
    // A start that is shorter than a bar still counts toward the downbeat.
    assert_eq!(link.write(&session, packet(3, 1, 7, 0), 3_000_000)[0], 0);
    assert_eq!(link.write(&session, packet(1, 2, 7, 1000), 3_000_000)[0], 0);
    session.advance(3_000_000);
    assert_eq!(session.count_in(), 3);
    session.advance(3_625_000);
    assert_eq!(session.count_in(), 4);
}

#[test]
fn the_status_sign_tells_stopped_counting_running_and_paused_apart() {
    use crate::core::model::ModelHandle;
    bench!(app, session);
    app.render().unwrap();
    let colors = Palette::of(app.world.resource::<Theme>().unwrap());
    let (x, y, _) = header_at(&app);
    // The sign is five pixels wide and nine high, one pixel in from the
    // corner. Stopped is a square, running a triangle, paused two bars.
    let probe = |app: &mut App<_, _>, column: i32, row: i32| {
        app.render().unwrap();
        pixel(&mut app.backend, x + 1 + column, y + 1 + row)
    };
    assert_eq!(probe(&mut app, 2, 4), rgb(colors.chrome), "stopped square");
    assert_eq!(
        probe(&mut app, 0, 0),
        rgb(colors.bg),
        "square has no corner"
    );

    session.toggle(0);
    session.advance(100_000);
    assert_eq!(probe(&mut app, 0, 0), rgb(colors.now), "running triangle");
    assert_eq!(probe(&mut app, 4, 4), rgb(colors.now), "running tip");

    session.pause(100_000);
    assert_eq!(probe(&mut app, 0, 0), rgb(colors.ink), "left bar");
    assert_eq!(probe(&mut app, 2, 0), rgb(colors.bg), "gap between bars");
    assert_eq!(probe(&mut app, 4, 0), rgb(colors.ink), "right bar");

    session.stop();
    assert_eq!(probe(&mut app, 2, 4), rgb(colors.chrome), "stopped again");
    assert!(ModelHandle::read(&session, |data| data.at_start));

    // Counting toward a delayed start shows the triangle, not yet lit.
    let mut link = DeviceLink::new(9);
    assert_eq!(link.write(&session, packet(1, 0, 9, 2500), 0)[0], 0);
    session.advance(0);
    assert_eq!(probe(&mut app, 0, 0), rgb(colors.chrome), "counting");
}

#[test]
fn the_dot_and_the_top_edge_follow_the_link_and_a_lost_link_clears_the_upload() {
    bench!(app, session);
    app.render().unwrap();
    let colors = Palette::of(app.world.resource::<Theme>().unwrap());
    let (x, y, width) = header_at(&app);
    // Five by five pixels at the right end of the row.
    let (dot_x, dot_y) = (x + width - 5, y + 4);
    let dot = |app: &mut App<_, _>, column: i32, row: i32| {
        app.render().unwrap();
        pixel(&mut app.backend, dot_x + column, dot_y + row)
    };
    let edge = |app: &mut App<_, _>, at: i32| {
        app.render().unwrap();
        pixel(&mut app.backend, x + at, y)
    };

    session.set_connected(true);
    assert_eq!(dot(&mut app, 2, 2), rgb(colors.ok), "connected is solid");
    session.set_connected(false);
    assert_eq!(dot(&mut app, 2, 2), rgb(colors.bg), "offline is hollow");
    assert_eq!(dot(&mut app, 0, 0), rgb(colors.chrome), "offline outline");

    // A song arriving: the dot turns the colour of things in flight and a
    // bar grows along the top edge.
    session.set_connected(true);
    session.set_upload(Some(40));
    assert_eq!(dot(&mut app, 2, 2), rgb(colors.queued));
    assert_eq!(edge(&mut app, 10), rgb(colors.queued), "bar, inside 40%");
    assert_eq!(edge(&mut app, 70), rgb(colors.bg), "bar, past 40%");

    // The link breaking ends the transfer; the bar must not stay behind.
    session.set_connected(false);
    assert_eq!(session.upload(), None);
    assert_eq!(edge(&mut app, 10), rgb(colors.bg));
}

#[test]
fn a_song_arriving_over_bluetooth_reports_how_much_of_it_is_in() {
    let mut app = App::headless(128, 128);
    let session = app
        .add_model(Session::new(
            Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap(),
        ))
        .finish();
    let source = "---\ntitle: Small\nkey: C\nbpm: 96\ntime: 4/4\nstrum: pop\n---\n--- Verse ---\n[Go] on @ 1\n".as_bytes();
    let mut link = DeviceLink::new(5);
    let mut begin = packet(0x10, 0, 5, source.len() as u32);
    begin[12..16].copy_from_slice(&checksum(source).to_le_bytes());
    assert_eq!(session.upload(), None);
    assert_eq!(link.write(&session, begin, 0)[0], 0);
    assert_eq!(session.upload(), Some(0));
    let mut seen = alloc::vec::Vec::new();
    for (index, data) in source.chunks(14).enumerate() {
        let mut chunk = [0u8; 20];
        chunk[0] = 0x11;
        chunk[1..3].copy_from_slice(&(index as u16 + 1).to_le_bytes());
        chunk[3..5].copy_from_slice(&((index * 14) as u16).to_le_bytes());
        chunk[5] = data.len() as u8;
        chunk[6..6 + data.len()].copy_from_slice(data);
        assert_eq!(link.write(&session, chunk, 0)[0], 0);
        seen.push(session.upload().unwrap());
    }
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]), "{seen:?}");
    assert_eq!(seen.last(), Some(&100));
    let end = packet(0x12, source.chunks(14).count() as u16 + 1, 5, 0);
    assert_eq!(link.write(&session, end, 0)[0], 0);
    assert_eq!(session.upload(), None);
    assert_eq!(session.title(), "Small");
}

#[test]
fn the_end_of_the_song_comes_in_from_the_right_on_the_chord_row() {
    bench!(app, session);
    let colors = Palette::of(app.world.resource::<Theme>().unwrap());
    // Counts the pixels in the label colour in the right part of the chord
    // row, where the next bar's first chord or the end of the song is.
    let label = |app: &mut App<_, _>| {
        app.render().unwrap();
        let sheet = app.world.find_by_id("prompter_sheet").unwrap();
        let row = app.world.get::<crate::ui::Children>(sheet).unwrap().0[0];
        let row = app.world.get::<crate::ui::ComputedRect>(row).unwrap().0;
        let top = row.y.to_int();
        let mut found = 0;
        for y in top..top + i32::from(super::font::CONTENT_SIZE) {
            for x in 98..124 {
                found += usize::from(pixel(&mut app.backend, x, y) == rgb(colors.chrome));
            }
        }
        found
    };
    session.toggle(0);
    // Three quarters into the first bar: the next chord is in the ink colour.
    session.advance(1_800_000);
    assert_eq!(label(&mut app), 0);
    // The same moment of the last bar, the eleventh after the first: END, not lit yet, comes in.
    session.advance(11 * 2_500_000 + 1_800_000);
    assert!(
        label(&mut app) > 20,
        "END must show before the last bar ends"
    );
}
