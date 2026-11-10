use super::*;

const SONG: &str = "---\ntitle: Practice\nkey: C\nbpm: 96.5\ntime: 4/4\nstrum: pop\n---\n--- Verse ---\n[跟]着[节]拍 | | [回]到起点 @ 1/5 6m 4\n--- Chorus | strum:whole ---\n[再]走一步 @ 1\n";

#[test]
fn song_preserves_musical_bars_unicode_and_section_overrides() {
    let score = Score::parse(SONG, ScoreLimits::DEVICE).unwrap();
    assert_eq!(score.title(), "Practice");
    assert_eq!(score.milli_bpm, 96_500);
    assert_eq!(score.bars(), 4);
    assert_eq!(score.chords().len(), 5);
    assert_eq!(score.text(score.chords()[0].lyric.clone()), "跟着");
    assert_eq!(score.text(score.chords()[1].lyric.clone()), "节拍");
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
    assert_eq!(session.lyric(), "跟着");
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
    let font = super::font::open();
    for ch in include_str!("../assets/prompter-charset.txt").chars() {
        assert!(font.map_char(ch).is_some(), "missing {ch}");
    }
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.load(SONG).unwrap();
    assert!(matches!(
        session.load(&SONG.replace("跟", "🦀")),
        Err(ScoreError::UnsupportedCharacter('🦀'))
    ));
    assert_eq!(session.lyric, "跟着");
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
        (super::font::chord(), 24, "C#Dmb"),
        (super::font::chord(), 36, "C#Dmb"),
        (super::font::chord(), 48, "C#Dmb"),
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
fn bar_motion_is_monotonic_pixel_snapped_and_continuous_at_the_handoff() {
    use crate::types::Fixed;
    let width = Fixed::from_int(120);
    let mut previous = 0;
    for phase in 0..=256 {
        let motion = BarMotion::at(phase, true);
        assert!(motion.travel >= previous && motion.travel <= 256);
        assert_eq!(motion.travel, phase, "horizontal speed must remain uniform");
        if phase > 2 {
            assert!(motion.travel > 0);
        }
        for offset in [-1, 0, 1] {
            let transform = motion.transform(width, offset);
            assert_eq!(transform.tx, transform.tx.round());
            assert_eq!(transform.m00, Fixed::ONE);
            assert_eq!(transform.m11, Fixed::ONE);
            assert!([24, 36, 48].contains(&super::motion::chord_size(phase, true, false, offset)));
        }
        assert_eq!(BarMotion::at(phase, false).travel, 0);
        previous = motion.travel;
    }
    assert_eq!(
        BarMotion::at(256, true).transform(width, 1),
        BarMotion::at(0, true).transform(width, 0)
    );
    assert_eq!(BarMotion::at(0, true).impact, 256);
    assert_eq!(BarMotion::at(192, true).impact, 0);
    assert_eq!(BarMotion::emphasis(0), 256);
    assert_eq!(BarMotion::emphasis(128), 0);
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
    assert_eq!(session.next_lyric, "");
    session.toggle(0);
    session.advance(1_500_000);
    assert_eq!(session.chord, "G");
    assert_eq!(session.lyric, "节拍");
    assert_eq!(session.next_chord, "Am");
    session.advance(2_350_000);
    assert!(BarMotion::at(session.bar_phase, true).travel > 0);
    session.pause(2_350_000);
    let phase = session.bar_phase;
    session.advance(20_000_000);
    assert_eq!(session.bar_phase, phase);
    session.stop();
    assert_eq!(session.bar_phase, 0);
    session.load(PRACTICE).unwrap();
    assert_eq!(session.stage_width, crate::types::Fixed::from_int(312));
    assert!(session.large);
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
        for name in ["prompter_chord", "prompter_lyric"] {
            let entity = app.world.find_by_id(name).unwrap();
            let rect = app.world.get::<crate::ui::ComputedRect>(entity).unwrap().0;
            assert_eq!(rect.x, Fixed::from_int(19));
            assert_eq!(rect.w, Fixed::from_int(90));
            assert_eq!(
                app.world
                    .get::<crate::ui::widgets::Text>(entity)
                    .unwrap()
                    .paragraph()
                    .align,
                crate::ui::widgets::TextAlign::Center
            );
        }
        let texture = app.backend.framebuffer();
        let pixels = texture.read_pixels().unwrap();
        let mut colors = alloc::collections::BTreeSet::new();
        for y in 4..44 {
            for x in 19..109 {
                let offset = y * pixels.stride() + x * 4;
                colors.insert(<[u8; 4]>::try_from(&pixels.bytes()[offset..offset + 4]).unwrap());
            }
        }
        assert_eq!(colors.len(), 2, "chord pixels must not be interpolated");
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
    let chord = app.world.find_by_id("prompter_chord").unwrap();
    let lyric = app.world.find_by_id("prompter_lyric").unwrap();
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
        assert_eq!(app.world.find_by_id("prompter_chord"), Some(chord));
        assert_eq!(app.world.find_by_id("prompter_lyric"), Some(lyric));
        let compact = width < 256 || height < 200;
        let controls = app.world.find_by_id("prompter_controls").unwrap();
        assert_eq!(app.world.has::<crate::ui::Hidden>(controls), compact);
        assert_eq!(
            app.world.get::<Style>(chord).unwrap().font_size,
            Some(if compact { 36 } else { 48 })
        );
        assert_eq!(
            app.world.get::<Style>(lyric).unwrap().font_size,
            Some(if compact { 12 } else { 24 })
        );
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
