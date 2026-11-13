use super::*;

/// The notes of the box on each string, as frets, lowest string first.
fn frets(board: &Fretboard) -> [[u8; 2]; STRINGS] {
    board.frets
}

#[test]
fn the_first_box_of_a_minor_in_the_key_of_c_is_the_familiar_one_at_the_fifth_fret() {
    let board = fretboard(0, Scale::Pentatonic, 1);
    assert_eq!(
        frets(&board),
        [[5, 8], [5, 7], [5, 7], [5, 7], [5, 8], [5, 8]]
    );
    assert_eq!((board.low, board.high), (5, Some(17)));
    // A is the root of both outer strings' first note.
    assert_eq!(board.notes[0][0], 9);
    assert_eq!(board.notes[5][0], 9);
}

#[test]
fn the_other_boxes_of_the_key_of_c_climb_the_neck() {
    let lows: alloc::vec::Vec<_> = (1..=5)
        .map(|position| fretboard(0, Scale::Pentatonic, position).low)
        .collect();
    assert_eq!(lows, [5, 7, 9, 0, 2]);
    // The fourth box is the open position of E minor; a box at the open
    // strings has a high home a full octave up.
    let open = fretboard(0, Scale::Pentatonic, 4);
    assert_eq!(open.high, Some(12));
    assert_eq!(
        frets(&open),
        [[0, 3], [0, 3], [0, 2], [0, 2], [1, 3], [0, 3]]
    );
}

#[test]
fn every_box_of_every_key_is_two_notes_to_a_string_within_five_frets_of_the_scale() {
    for key in 0..12 {
        for scale in [Scale::Pentatonic, Scale::Minor] {
            let allowed = scale.notes(key);
            for position in 1..=POSITIONS {
                let board = fretboard(key, scale, position);
                let top = board.frets.iter().flatten().copied().max().unwrap();
                assert!(top - board.low <= 4, "key {key} {scale:?} box {position}");
                for string in 0..STRINGS {
                    let [first, second] = board.frets[string];
                    assert!(first < second, "key {key} {scale:?} box {position}");
                    for slot in 0..2 {
                        let pitch = OPEN[string] + board.frets[string][slot];
                        assert_eq!(board.notes[string][slot], (pitch + LOW_E) % 12);
                        assert!(allowed.contains(&board.notes[string][slot]));
                    }
                }
                if let Some(high) = board.high {
                    assert!(top + 12 <= NECK_FRETS && high == board.low + 12);
                }
            }
        }
    }
}

#[test]
fn the_boxes_cover_every_note_of_the_scale_on_every_string() {
    for key in 0..12 {
        let mut seen = [[0u16; 1]; STRINGS];
        for position in 1..=POSITIONS {
            let board = fretboard(key, Scale::Pentatonic, position);
            for string in 0..STRINGS {
                for note in board.notes[string] {
                    seen[string][0] |= 1 << note;
                }
            }
        }
        for string in 0..STRINGS {
            assert_eq!(seen[string][0].count_ones(), 5, "key {key} string {string}");
        }
    }
}

#[test]
fn the_minor_scale_names_its_first_box_for_the_key_itself() {
    // C minor pentatonic: the root C on the sixth string is at the eighth fret.
    let board = fretboard(0, Scale::Minor, 1);
    assert_eq!(board.low, 8);
    assert_eq!(board.notes[0][0], 0);
}

#[test]
fn a_chord_names_its_root_and_the_other_notes_of_its_triad() {
    let a_minor = Tones::new(9, true);
    assert_eq!(a_minor.role(9), Role::Root);
    assert_eq!(a_minor.role(0), Role::Tone);
    assert_eq!(a_minor.role(4), Role::Tone);
    assert_eq!(a_minor.role(2), Role::Scale);
    let g = Tones::new(7, false);
    assert_eq!(
        [g.role(7), g.role(11), g.role(2), g.role(0)],
        [Role::Root, Role::Tone, Role::Tone, Role::Scale]
    );
}

#[test]
fn names_round_trip() {
    for scale in [Scale::Pentatonic, Scale::Minor] {
        assert_eq!(Scale::parse(scale.name()), Some(scale));
    }
    assert_eq!(Scale::parse("blues"), None);
}
