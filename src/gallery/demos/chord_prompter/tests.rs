use super::*;

const SONG: &str = "---\ntitle: Practice\nkey: C\nbpm: 96.5\ntime: 4/4\nstrum: pop\n---\n--- Verse ---\n[跟]着[节]拍 | | [回]到起点 @ 1/5 6m 4\n--- Chorus | strum:whole ---\n[再]走一步 @ 1\n";

#[test]
fn source_accents_are_borrowed_with_plain_utf8_offsets_without_losing_bar_content() {
    let score = Score::parse(SONG, ScoreLimits::DEVICE).unwrap();
    let bar = score.bar_chords(0);
    assert_eq!(bar.len(), 2);
    let first: alloc::vec::Vec<_> = score.lyric(&bar[0]).segments().collect();
    assert_eq!(
        first,
        [
            LyricSegment {
                text: "跟",
                accented: true,
                offset: 0
            },
            LyricSegment {
                text: "着",
                accented: false,
                offset: 3
            }
        ]
    );
    let text: alloc::string::String = bar
        .iter()
        .flat_map(|chord| score.lyric(chord).chars())
        .collect();
    assert_eq!(text, "跟着节拍");
    let empty = Score::parse(
        &SONG.replace("[再]走一步", "[]A[再][]B[走]"),
        ScoreLimits::DEVICE,
    )
    .unwrap();
    assert_eq!(
        alloc::format!("{}", empty.lyric(&empty.bar_chords(3)[0])),
        "A再B走"
    );
    assert_eq!(
        empty
            .lyric(&empty.bar_chords(3)[0])
            .segments()
            .filter(|span| span.accented)
            .count(),
        2
    );
}

#[test]
fn whole_bar_wrapping_retains_every_character_accent_and_timed_column() {
    let score = Score::parse(SONG, ScoreLimits::DEVICE).unwrap();
    let mut sheet = super::sheet::Sheet::new();
    sheet.rebuild(&score, 0, 120, 80);
    assert_eq!(sheet.text, "跟着节拍");
    assert_eq!(sheet.current_count, 2);
    assert_eq!(sheet.slots[0].width, sheet.slots[1].width);
    assert!(sheet.accented(0));
    assert!(!sheet.accented(3));
    assert!(sheet.accented(6));
    let plain_cap = sheet.text.capacity();
    let line_cap = sheet.lines.capacity();
    for bar in 0..score.bars() {
        sheet.rebuild(&score, bar, 120, 80);
    }
    assert_eq!(sheet.text.capacity(), plain_cap);
    assert_eq!(sheet.lines.capacity(), line_cap);
}

#[test]
fn long_bars_remain_accepted_and_vertical_follow_reaches_the_final_line() {
    use crate::types::Fixed;
    let source = PRACTICE.replace(
        "[Keep] the rhythm",
        &alloc::format!("[A]{}Z", "x".repeat(120)),
    );
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.sheet_viewport(Fixed::from_int(120), Fixed::from_int(76));
    session.load(&source).unwrap();
    assert_eq!(session.sheet.current_text().len(), 122);
    assert!(session.sheet.current_text().ends_with('Z'));
    assert!(session.content_height > Fixed::from_int(76));
    let lines = &session.sheet.lines;
    let last =
        &lines[usize::from(session.sheet.slots[session.sheet.current_count - 1].end_line) - 1];
    assert_eq!(usize::from(last[1]), session.sheet.current_text().len());
    session.toggle(0);
    session.advance(2_490_000);
    assert!(session.sheet_scroll > Fixed::ZERO);
    let max = session.content_height - Fixed::from_int(76);
    assert!(max - session.sheet_scroll <= Fixed::from_int(2));
    session.pause(2_490_000);
    let offset = session.sheet_scroll;
    let revision = session.sheet_revision;
    session.advance(20_000_000);
    assert_eq!(session.sheet_scroll, offset);
    assert_eq!(session.sheet_revision, revision);
}

#[test]
fn horizontal_pre_reading_and_vertical_follow_remain_independent() {
    use crate::types::Fixed;
    let source = PRACTICE.replace(
        "[Keep] the rhythm",
        &alloc::format!("[A]{}Z", "x".repeat(120)),
    );
    let mut session = Session::new(Score::parse(source.as_str(), ScoreLimits::DEVICE).unwrap());
    session.sheet_viewport(Fixed::from_int(120), Fixed::from_int(72));
    assert!(session.next_content_height < Fixed::from_int(72));
    assert_eq!(session.sheet.slots[session.sheet.current_count].y, 0);
    session.toggle(0);
    session.advance(625_000);
    assert_eq!(session.page_scroll, Fixed::from_int(90));
    assert!(session.sheet_scroll > Fixed::ZERO);
    assert_eq!(session.sheet.slots[session.sheet.current_count].y, 0);
    session.pause(625_000);
    let horizontal = session.page_scroll;
    let vertical = session.sheet_scroll;
    session.advance(20_000_000);
    assert_eq!(session.page_scroll, horizontal);
    assert_eq!(session.sheet_scroll, vertical);
    session.stop();
    assert_eq!(session.page_scroll, Fixed::from_int(60));
    assert_eq!(session.sheet_scroll, Fixed::ZERO);
}

#[test]
fn chord_onsets_and_the_fixed_needle_share_the_viewport_center() {
    use crate::prelude::*;
    use crate::surface::FramebufferAccess;
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    session.toggle(0);
    session.advance(11_250_000);
    app.render().unwrap();
    assert_eq!(session.chord(), "G");
    let label = app.world.find_by_id("prompter_bar").unwrap();
    let rect = app.world.get::<crate::ui::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.x + rect.w, Fixed::from_int(124));
    let color = app
        .world
        .resource::<crate::ui::Theme>()
        .unwrap()
        .resolve(ColorToken::Primary);
    let texture = app.backend.framebuffer();
    let pixels = texture.read_pixels().unwrap();
    let mut bounds = None::<(usize, usize)>;
    let mut chord_ink = 0;
    for y in 22..36 {
        for x in 4..124 {
            let at = y * pixels.stride() + x * 4;
            if pixels.bytes()[at..at + 3] == [color.r, color.g, color.b] {
                if x.abs_diff(64) > 2 {
                    chord_ink += 1;
                }
                bounds = Some(bounds.map_or((x, x), |(left, right)| (left.min(x), right.max(x))));
            }
        }
    }
    let (left, right) = bounds.expect("active rolling chord ink");
    assert!(chord_ink > 4, "the fixed needle alone is not chord ink");
    assert!(
        (left + right).abs_diff(128) <= 4,
        "rolling chord must trigger around x=64, got {left}..{right}"
    );
}

#[test]
fn incremental_musical_scroll_matches_full_redraw_at_onsets_and_overflow() {
    use crate::prelude::*;
    use crate::surface::FramebufferAccess;
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    for source in [
        PRACTICE.to_string(),
        PRACTICE.replace("[Keep] the rhythm", &"x".repeat(120)),
    ] {
        session.load(&source).unwrap();
        session.toggle(0);
        app.render().unwrap();
        for time in [
            1, 625_000, 1_249_999, 1_250_000, 2_499_999, 2_500_000, 11_250_000,
        ] {
            session.advance(time);
            app.render_dirty().unwrap();
            let expected = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            let texture = app.backend.framebuffer();
            let pixels = texture.read_pixels().unwrap();
            if let Some(pixel) = expected
                .chunks_exact(4)
                .zip(pixels.bytes().chunks_exact(4))
                .position(|(a, b)| a != b)
            {
                panic!(
                    "scroll damage differs at {time}, pixel ({}, {}): incremental {:?}, full {:?}",
                    pixel % 128,
                    pixel / 128,
                    &expected[pixel * 4..pixel * 4 + 4],
                    &pixels.bytes()[pixel * 4..pixel * 4 + 4]
                );
            }
        }
        session.pause(11_250_000);
        app.render_dirty().unwrap();
        let viewport = app.world.find_by_id("prompter_sheet_viewport").unwrap();
        for x in [100, 120, 180] {
            app.world
                .get_mut::<crate::input::event::scroll::ScrollOffset>(viewport)
                .unwrap()
                .x = Fixed::from_int(x);
            app.world.invalidate_visual(viewport);
            app.render_dirty().unwrap();
            let expected = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            assert!(
                expected == app.backend.framebuffer().read_pixels().unwrap().bytes(),
                "paused manual scroll differs at {x}"
            );
        }
    }
}

#[test]
fn scheduled_start_does_not_trigger_marks_until_the_music_clock_starts() {
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.play_at(0, 500);
    assert!(!session.clock_active);
    session.advance(499_000);
    assert!(!session.clock_active);
    session.advance(500_000);
    assert!(session.clock_active);
    session.pause(500_000);
    assert!(!session.clock_active);
}

#[test]
fn thousands_of_characters_in_one_bar_scroll_without_fragment_limits_or_hot_growth() {
    use crate::types::Fixed;
    let source = PRACTICE.replace(
        "[Keep] the rhythm",
        &alloc::format!("[A]{}Z", "x".repeat(7000)),
    );
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.sheet_viewport(Fixed::from_int(120), Fixed::from_int(76));
    session.load(&source).unwrap();
    assert_eq!(session.sheet.current_text().len(), 7002);
    let text_capacity = session.sheet.text.capacity();
    let line_capacity = session.sheet.lines.capacity();
    session.toggle(0);
    for time in [500_000, 1_000_000, 2_499_999, 2_500_000, 15_000_000] {
        session.advance(time);
        assert_eq!(session.sheet.text.capacity(), text_capacity);
        assert_eq!(session.sheet.lines.capacity(), line_capacity);
    }
}

#[test]
fn song_preserves_musical_bars_unicode_and_section_overrides() {
    let score = Score::parse(SONG, ScoreLimits::DEVICE).unwrap();
    assert_eq!(score.title(), "Practice");
    assert_eq!(score.milli_bpm, 96_500);
    assert_eq!(score.bars(), 4);
    assert_eq!(score.chords().len(), 5);
    assert!(score.lyric(&score.chords()[0]).chars().eq("跟着".chars()));
    assert!(score.lyric(&score.chords()[1]).chars().eq("节拍".chars()));
    assert!(score.text(score.chords()[2].lyric.clone()).is_empty());
    assert_eq!(score.chord_name(1), "C");
    assert_eq!(score.chord_name(5), "G");
    assert_eq!(score.chord_name(6), "Am");
    assert_eq!(score.section_at(2), 0);
    assert_eq!(score.section_at(3), 1);
    assert_eq!(score.sections()[1].pattern, Pattern::Whole);
    assert_eq!(score.chord_at(1919), 0);
    assert_eq!(score.chord_at(1920), 1);
    assert_eq!(score.chord_at(3840), 2);
    assert_eq!(score.chord_at(u64::MAX), 4);
}

#[test]
fn importer_rejects_invalid_input_and_enforces_all_limits() {
    for key in [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ] {
        assert!(
            Score::parse(
                &SONG.replace("key: C", &alloc::format!("key: {key}")),
                ScoreLimits::DEVICE
            )
            .is_ok()
        );
    }
    for invalid in [
        SONG.replace("1/5", "1/7"),
        SONG.replace("[跟]着[节]拍", "[跟]着节拍"),
        SONG.replace("[跟]", "[[跟]]"),
        SONG.replace("[跟]", "[跟"),
        SONG.replace("96.5", "NaN"),
        SONG.replace("96.5", "0"),
        SONG.replace("4/4", "4/3"),
        SONG.replace("strum: pop", "strum: missing"),
    ] {
        assert!(
            Score::parse(&invalid, ScoreLimits::DEVICE).is_err(),
            "{invalid}"
        );
    }
    for limits in [
        ScoreLimits {
            bytes: 3,
            ..ScoreLimits::DEVICE
        },
        ScoreLimits {
            bars: 3,
            ..ScoreLimits::DEVICE
        },
        ScoreLimits {
            chords: 4,
            ..ScoreLimits::DEVICE
        },
        ScoreLimits {
            sections: 1,
            ..ScoreLimits::DEVICE
        },
    ] {
        assert!(matches!(
            Score::parse(SONG, limits),
            Err(ScoreError::Capacity)
        ));
    }
    assert!(Score::parse(&SONG.replace('\n', "\r\n"), ScoreLimits::DEVICE).is_ok());
}

#[test]
fn uneven_chord_subdivision_uses_the_same_bar_phase() {
    let source = SONG
        .replace("1/5", "1/5/6m")
        .replace("[跟]着[节]拍", "[跟]着[节][拍]");
    let score = Score::parse(&source, ScoreLimits::DEVICE).unwrap();
    assert_eq!(score.chord_at(1279), 0);
    assert_eq!(score.chord_at(1280), 1);
    assert_eq!(score.chord_at(2559), 1);
    assert_eq!(score.chord_at(2560), 2);
    for pattern in [Pattern::Pop, Pattern::Shuffle, Pattern::Arpeggio] {
        let attacks = pattern.attacks();
        assert!(
            attacks
                .windows(2)
                .all(|pair| pair[0].offset < pair[1].offset)
        );
        assert!(
            attacks
                .iter()
                .all(|hit| hit.offset < 400 && hit.strings[1] <= 5)
        );
    }
}

#[test]
fn elapsed_time_is_independent_of_frame_cadence() {
    let mut dense = Transport::new(96_500, 4).unwrap();
    let mut sparse = Transport::new(96_500, 4).unwrap();
    dense.start(1_000_000);
    sparse.start(1_000_000);
    for time in (1_000_000..601_000_000).step_by(16_667) {
        dense.advance(time);
    }
    for time in (1_000_000..601_000_000).step_by(173_333) {
        sparse.advance(time);
    }
    assert_eq!(dense.advance(601_000_000), 926_400);
    assert_eq!(dense.position(), sparse.advance(601_000_000));
    sparse.pause(601_000_000);
    assert_eq!(sparse.advance(9_000_000_000), 926_400);
    sparse.start(9_000_000_000);
    assert_eq!(sparse.advance(9_060_000_000), 1_019_040);
}

#[test]
fn tempo_changes_take_effect_on_the_boundary_despite_late_frames() {
    let mut transport = Transport::new(120_000, 4).unwrap();
    transport.start(0);
    assert_eq!(
        transport.schedule(100_000, Boundary::Beat, Action::Tempo(60_000)),
        Ok(960)
    );
    assert_eq!(transport.advance(499_999), 959);
    assert_eq!(transport.milli_bpm(), 120_000);
    assert_eq!(transport.advance(1_500_000), 1920);
    assert_eq!(transport.milli_bpm(), 60_000);
    assert_eq!(transport.pending(), 0);
}

#[test]
fn pause_preserves_fractional_phase_and_seek_retires_old_controls() {
    let mut transport = Transport::new(120_000, 4).unwrap();
    transport.start(0);
    transport.pause(125_000);
    assert_eq!(transport.position(), 240);
    transport.start(10_000_000);
    assert_eq!(transport.advance(10_125_000), 480);
    transport
        .schedule(10_125_000, Boundary::Bar, Action::Seek(7680))
        .unwrap();
    transport
        .schedule(10_125_000, Boundary::Bar, Action::Tempo(60_000))
        .unwrap();
    assert_eq!(transport.advance(12_125_000), 8160);
    assert_eq!(transport.milli_bpm(), 120_000);
    assert_eq!(transport.pending(), 0);
    transport.stop();
    assert_eq!(transport.playback(), Playback::Stopped);
    assert_eq!(transport.position(), 0);
}

#[test]
fn control_queue_is_bounded_and_sorted_without_allocations() {
    let mut transport = Transport::new(120_000, 4).unwrap();
    transport.start(0);
    transport
        .schedule(0, Boundary::Bar, Action::Tempo(90_000))
        .unwrap();
    transport
        .schedule(0, Boundary::Beat, Action::Tempo(60_000))
        .unwrap();
    assert_eq!(transport.advance(1_500_000), 1920);
    assert_eq!(transport.milli_bpm(), 60_000);
    assert_eq!(transport.advance(3_500_000), 3840);
    assert_eq!(transport.milli_bpm(), 90_000);
    for _ in 0..8 {
        transport
            .schedule(3_500_000, Boundary::Bar, Action::Tempo(90_000))
            .unwrap();
    }
    assert_eq!(
        transport.schedule(3_500_000, Boundary::Beat, Action::Tempo(100_000)),
        Err(CommandError::Capacity)
    );
    assert_eq!(
        transport.schedule(3_500_000, Boundary::Now, Action::Tempo(0)),
        Err(CommandError::Tempo)
    );
}

#[test]
fn session_preserves_score_on_failed_import_and_stops_at_the_end() {
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    assert_eq!(session.chord, "C");
    assert_eq!(session.next_chord, "G");
    session.toggle(0);
    session.advance(2_500_000);
    assert_eq!(session.chord, "G");
    assert!(session.load("incomplete").is_err());
    assert_eq!(session.chord, "G");
    session.advance(100_000_000);
    assert!(!session.playing);
    assert_eq!(session.chord, "C");
    assert_eq!(session.next_chord, "END");
    session.stop();
    assert_eq!(session.bar_phase, 0);
    assert_eq!(session.chord, "C");
}

#[test]
fn shared_composition_renders_small_and_large_without_text_overflow() {
    use crate::prelude::*;
    for (width, height) in [(128, 128), (360, 480), (480, 320)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        app.add_resource(MonoClock::new(|| 0));
        let root = app.spawn_root().id();
        let session = install(&mut app, root);
        app.render().unwrap();
        crate::gallery::demos::assert_text_layouts_fit(&app.world);
        session.toggle(0);
        for time in [310_000, 625_000, 1_875_000, 2_500_000, 10_000_000] {
            session.advance(time);
            app.render_dirty().unwrap();
            crate::gallery::demos::assert_text_layouts_fit(&app.world);
        }
    }
}

fn packet(opcode: u8, seq: u16, generation: u32, arg: u32) -> [u8; 20] {
    let mut packet = [0u8; 20];
    packet[0] = opcode;
    packet[1..3].copy_from_slice(&seq.to_le_bytes());
    packet[3..7].copy_from_slice(&generation.to_le_bytes());
    packet[8..12].copy_from_slice(&arg.to_le_bytes());
    packet
}

#[test]
fn link_orders_commands_deduplicates_retries_and_checks_source_identity() {
    use crate::prelude::*;
    let mut app = App::headless(128, 128);
    let session = app
        .add_model(Session::new(
            Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap(),
        ))
        .finish();
    let mut link = DeviceLink::new(17);
    let start = packet(1, 0, 17, 500);
    let receipt = link.write(&session, start, 1_000_000);
    assert_eq!(receipt[0], 0);
    assert_eq!(
        u32::from_le_bytes(receipt[12..16].try_into().unwrap()),
        1500
    );
    assert_eq!(link.status(&session, 1_100_000)[1] & 4, 4);
    assert_eq!(session.position(), 0);
    assert_eq!(
        link.write(&session, packet(1, 3, 17, 0), 1_100_000)[0],
        LinkError::Sequence as u8
    );
    assert_eq!(link.write(&session, start, 1_200_000), receipt);
    assert_eq!(session.starts_at_us(), 1_500_000);
    assert_eq!(
        link.write(&session, packet(3, 1, 16, 0), 1_300_000)[0],
        LinkError::Generation as u8
    );
    assert!(session.playing());
    assert_eq!(link.write(&session, packet(3, 2, 17, 0), 1_400_000)[0], 0);
    assert!(!session.playing());
    link.reconnect();
    assert_eq!(link.write(&session, packet(1, 0, 17, 0), 2_000_000)[0], 0);
}

#[test]
fn transfer_validates_bytes_before_atomic_score_replacement() {
    use crate::prelude::*;
    let mut app = App::headless(128, 128);
    let session = app
        .add_model(Session::new(
            Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap(),
        ))
        .finish();
    let mut link = DeviceLink::new(23);
    let source = SONG.as_bytes();
    let mut begin = packet(0x10, 0, 23, source.len() as u32);
    begin[12..16].copy_from_slice(&checksum(source).to_le_bytes());
    assert_eq!(link.write(&session, begin, 0)[0], 0);
    assert_eq!(
        link.write(&session, packet(0x12, 1, 23, 0), 0)[0],
        LinkError::Transfer as u8
    );
    assert_eq!(session.title(), "Midnight Session");
    let mut seq: u16 = 2;
    for (offset, data) in source.chunks(14).enumerate() {
        let mut chunk = [0u8; 20];
        chunk[0] = 0x11;
        chunk[1..3].copy_from_slice(&seq.to_le_bytes());
        chunk[3..5].copy_from_slice(&((offset * 14) as u16).to_le_bytes());
        chunk[5] = data.len() as u8;
        chunk[6..6 + data.len()].copy_from_slice(data);
        assert_eq!(link.write(&session, chunk, 0)[0], 0);
        seq += 1;
    }
    assert_eq!(session.title(), "Midnight Session");
    let receipt = link.write(&session, packet(0x12, seq, 23, 0), 0);
    assert_eq!(receipt[0], 0);
    assert_eq!(u32::from_le_bytes(receipt[4..8].try_into().unwrap()), 24);
    assert_eq!(session.title(), "Practice");
    assert_eq!(
        crate::core::model::ModelHandle::read(&session, |data| data
            .sheet
            .current_text()
            .to_string()),
        "跟着节拍"
    );
    assert_eq!(
        link.write(&session, packet(1, seq + 1, 23, 0), 0)[0],
        LinkError::Generation as u8
    );
}

#[test]
fn explicit_late_controls_are_rejected_and_rhythm_changes_share_the_bar_clock() {
    let mut transport = Transport::new(120_000, 4).unwrap();
    transport.start(0);
    transport.advance(1_000_000);
    assert_eq!(
        transport.schedule_at(1_000_000, 960, Action::Tempo(60_000)),
        Err(CommandError::Late)
    );
    transport
        .schedule_at(1_000_000, 3840, Action::Rhythm(Some(Pattern::Rock)))
        .unwrap();
    transport.advance(2_500_000);
    assert_eq!(transport.rhythm(), Some(Pattern::Rock));
    assert_eq!(transport.position(), 4800);
}

#[test]
fn bundled_charset_is_complete_and_missing_glyphs_preserve_the_active_score() {
    let font = super::font::body();
    for ch in include_str!("../assets/prompter-charset.txt").chars() {
        assert!(font.map_char(ch).is_some(), "missing {ch}");
    }
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.load(SONG).unwrap();
    assert!(matches!(
        session.load(&SONG.replace("跟", "🦀")),
        Err(ScoreError::UnsupportedCharacter('🦀'))
    ));
    assert_eq!(session.sheet.current_text(), "跟着节拍");
}

#[test]
fn all_rhythm_presentations_use_the_shared_timeline_on_the_small_display() {
    use crate::prelude::*;
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    for (index, pattern) in Pattern::ALL.iter().enumerate() {
        session
            .load(&PRACTICE.replace("strum: pop", &alloc::format!("strum: {}", pattern.name())))
            .unwrap();
        session.toggle(0);
        session.advance(100_000 + index as u64 * 60_000);
        app.render_dirty().unwrap();
        crate::gallery::demos::assert_text_layouts_fit(&app.world);
        for attack in pattern.attacks() {
            assert!(attack.strings[0] <= attack.strings[1]);
            if attack.stroke == Stroke::Pick {
                assert_eq!(attack.strings[0], attack.strings[1]);
            }
        }
    }
}

#[test]
fn prompter_fonts_use_native_one_bit_strikes_at_every_requested_size() {
    use mirx::image::SampleLayout;
    for (font, size, text) in [
        (super::font::body(), 12, "跟着节拍"),
        (super::font::body(), 24, "跟着节拍"),
        (super::font::body(), 12, "C#Dmb"),
    ] {
        for ch in text.chars() {
            let glyph = font.map_char(ch).unwrap();
            let raster = font.raster(glyph, size).unwrap();
            assert_eq!(raster.representation.design_ppem(), size);
            assert_eq!(raster.surface.sample_layout(), SampleLayout::A1);
        }
    }
}

#[test]
fn center_emphasis_is_bounded_symmetric_and_largest_at_the_center() {
    assert_eq!(center_emphasis(0), 256);
    let mut last = 256;
    for offset in 0..512 {
        let value = center_emphasis(offset);
        assert_eq!(value, center_emphasis(-offset));
        assert!(value <= last);
        last = value;
    }
}

#[test]
fn upcoming_card_uses_the_next_bar_while_split_chords_keep_their_actual_timing() {
    let mut session = Session::new(Score::parse(SONG, ScoreLimits::DEVICE).unwrap());
    session.resize(
        crate::types::Fixed::from_int(320),
        crate::types::Fixed::from_int(240),
    );
    assert_eq!(session.chord, "C");
    assert_eq!(session.next_chord, "Am");
    session.toggle(0);
    session.advance(1_500_000);
    assert_eq!(session.chord, "G");
    assert_eq!(session.sheet.current_text(), "跟着节拍");
    assert_eq!(session.next_chord, "Am");
    session.advance(2_350_000);
    assert!(session.bar_phase > 0);
    session.pause(2_350_000);
    let phase = session.bar_phase;
    session.advance(20_000_000);
    assert_eq!(session.bar_phase, phase);
    session.stop();
    assert_eq!(session.bar_phase, 0);
    session.load(PRACTICE).unwrap();
    assert_eq!(session.stage_width, crate::types::Fixed::from_int(312));
}

#[test]
fn connection_feedback_survives_score_replacement_and_uses_binary_pixels() {
    use crate::prelude::*;
    use crate::surface::FramebufferAccess;
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    let mut frames = alloc::vec::Vec::new();
    for connected in [false, true] {
        session.set_connected(connected);
        session.load(PRACTICE).unwrap();
        assert_eq!(session.connected(), connected);
        app.render_dirty().unwrap();
        let texture = app.backend.framebuffer();
        let pixels = texture.read_pixels().unwrap();
        frames.push(pixels.bytes().to_vec());
    }
    let changed = frames[0]
        .chunks_exact(4)
        .zip(frames[1].chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        changed, 9,
        "connection state only changes the 3x3 indicator"
    );
}

#[test]
fn resizing_reuses_entities_and_matches_direct_creation_at_every_size() {
    use crate::prelude::*;
    use crate::surface::{FramebufferAccess, framebuf::FramebufSurface};
    let mut app = App::headless(320, 240);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    session.load(SONG).unwrap();
    app.render().unwrap();
    let sheet = app.world.find_by_id("prompter_sheet").unwrap();
    for (width, height) in [
        (128, 128),
        (320, 240),
        (480, 320),
        (128, 320),
        (640, 128),
        (1024, 768),
        (128, 128),
    ] {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        app.backend = FramebufSurface::new(width, height, flush);
        app.render().unwrap();
        crate::gallery::demos::assert_text_layouts_fit(&app.world);
        assert_eq!(app.world.find_by_id("prompter_sheet"), Some(sheet));
        let compact = width < 256 || height < 200;
        let controls = app.world.find_by_id("prompter_controls").unwrap();
        assert_eq!(app.world.has::<crate::ui::Hidden>(controls), compact);
        let actual = app.backend.framebuffer();
        let actual = actual.read_pixels().unwrap();

        let mut direct = App::headless(width, height);
        direct.with_default_widgets().with_default_systems();
        direct.add_resource(MonoClock::new(|| 0));
        let root = direct.spawn_root().id();
        let session = install(&mut direct, root);
        session.load(SONG).unwrap();
        direct.render().unwrap();
        let expected = direct.backend.framebuffer();
        let expected = expected.read_pixels().unwrap();
        assert_eq!(
            actual.bytes(),
            expected.bytes(),
            "resized output differs at {width}x{height}"
        );
    }
}
