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
