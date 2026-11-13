use super::*;
use crate::gallery::demos::chord_prompter::{PRACTICE, Score, ScoreLimits};

fn mark(plain: i32, advance: i32) -> Mark {
    Mark {
        plain: Fixed::from_int(plain),
        advance: Fixed::from_int(advance),
        ..Mark::default()
    }
}

#[test]
fn a_chord_waits_at_its_bar_position_until_the_needle_reaches_it() {
    let marks = [mark(100, 12), mark(160, 12)];
    for needle in [-50, 0, 99, 100] {
        assert_eq!(
            place(&marks, 0, Fixed::from_int(needle)),
            Fixed::from_int(100)
        );
    }
}

#[test]
fn a_chord_the_needle_has_passed_stays_on_the_needle() {
    let marks = [mark(100, 12), mark(160, 12)];
    for needle in [101, 120, 140] {
        assert_eq!(
            place(&marks, 0, Fixed::from_int(needle)),
            Fixed::from_int(needle)
        );
    }
}

#[test]
fn the_next_chord_pushes_it_aside_and_then_keeps_its_distance() {
    let marks = [mark(100, 12), mark(160, 12)];
    // Its name ends a gap before the coming chord begins.
    let aside = Fixed::from_int(160 - 12 - MARK_GAP);
    assert_eq!(place(&marks, 0, Fixed::from_int(146)), aside);
    assert_eq!(place(&marks, 0, Fixed::from_int(400)), aside);
}

#[test]
fn the_last_chord_and_the_end_of_the_song_follow_the_needle_without_limit() {
    let marks = [mark(100, 12), mark(160, 36)];
    assert_eq!(place(&marks, 1, Fixed::from_int(40)), Fixed::from_int(160));
    assert_eq!(place(&marks, 1, Fixed::from_int(500)), Fixed::from_int(500));
}

#[test]
fn chords_are_continuous_in_time_never_run_backwards_and_never_overlap_when_there_is_room() {
    // Names of two and three characters, in a bar that holds four of them.
    let marks = [mark(100, 24), mark(130, 12), mark(160, 36), mark(190, 12)];
    for index in 0..marks.len() {
        let mut last = place(&marks, index, Fixed::from_int(-40));
        for step in -39..400 {
            let at = place(&marks, index, Fixed::from_int(step));
            assert!(at >= last, "chord {index} runs backwards at {step}");
            assert!(
                at - last <= Fixed::ONE,
                "chord {index} jumps by more than the needle at {step}"
            );
            last = at;
            if let Some(next) = marks.get(index + 1) {
                let roomy = marks[index].plain + marks[index].advance + Fixed::from_int(MARK_GAP)
                    <= next.plain;
                if roomy {
                    assert!(
                        at + marks[index].advance + Fixed::from_int(MARK_GAP) <= next.plain,
                        "chord {index} touches the next at {step}"
                    );
                }
            }
        }
    }
}

#[test]
fn chords_do_not_jump_where_the_pages_behind_them_are_replaced() {
    let mut font = crate::gallery::demos::chord_prompter::font::body();
    font.size = crate::gallery::demos::chord_prompter::font::CONTENT_SIZE;
    let mut session = Session::new(Score::parse(PRACTICE, ScoreLimits::DEVICE).unwrap());
    session.sheet_viewport(Fixed::from_int(120), Fixed::from_int(76));
    session.toggle(0);
    // Where each chord of the visible stretch is drawn, left to right.
    let drawn = |session: &mut Session, now_us: u64| {
        session.advance(now_us);
        let (marks, count) = collect(session, &font, Fixed::ZERO);
        let marks = &marks[..count];
        let needle = needle_at(session, Fixed::ZERO);
        let mut shown: alloc::vec::Vec<_> = (0..count)
            .map(|index| place(marks, index, needle) - session.page_scroll)
            .filter(|x| *x > Fixed::from_int(-20) && *x < Fixed::from_int(130))
            .collect();
        shown.sort();
        shown
    };
    // The bar line between bars one and two, and the one inside the chorus
    // after a bar of two chords.
    for line in [2_500_000, 12_500_000] {
        let before = drawn(&mut session, line - 1_000);
        let after = drawn(&mut session, line + 1_000);
        assert_eq!(before.len(), after.len(), "a chord appeared or vanished");
        for (a, b) in before.iter().zip(&after) {
            assert!(
                (*a - *b).abs() <= Fixed::ONE,
                "a chord moved from {} to {} over the bar line",
                a.to_int(),
                b.to_int()
            );
        }
    }
}
