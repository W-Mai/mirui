//! Words the expedition games share, in English and Chinese.

use super::{Direction4, EXPEDITION_LEVELS, ExpeditionModal, ExpeditionProgress};
use crate::core::i18n::{Locale, Phrase, locale};
use crate::tr;
use core::fmt;

/// Whether the current, observed language is Chinese.
pub(crate) fn chinese() -> bool {
    matches!(locale(), Locale::ZhCn)
}

pub(crate) const MAP_TITLE: Phrase = tr!(en: "EXPEDITION MAP", zh: "远征航图");
pub(crate) const MAP_FOOTER: Phrase = tr!(
    en: "Clear a level to open the next one",
    zh: "完成一关，开启下一关"
);
pub(crate) const START_CHAPTER: Phrase = tr!(en: "Start chapter →", zh: "开始这一章 →");

/// The title of a result card: the last level, the last of a chapter, or
/// any other one (`cleared`, in the game's own words).
pub(crate) const fn result_title(modal: ExpeditionModal, level: u8, cleared: Phrase) -> Phrase {
    match modal {
        ExpeditionModal::Final => tr!(en: "EXPEDITION COMPLETE", zh: "远征完成"),
        _ if level % 6 == 5 => tr!(en: "CHAPTER CLEAR", zh: "章节完成"),
        _ => cleared,
    }
}

/// The main button of a result card: on to the next level, or to the log
/// once the last level is done.
pub(crate) const fn result_next(modal: ExpeditionModal, next: Phrase) -> Phrase {
    if matches!(modal, ExpeditionModal::Final) {
        tr!(en: "See log →", zh: "查看档案 →")
    } else {
        next
    }
}

/// Three stars, the earned ones filled.
struct Stars(u8);

impl fmt::Display for Stars {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for slot in 0..3 {
            output.write_str(if slot < self.0 { "★" } else { "☆" })?;
        }
        Ok(())
    }
}

/// Moves made against the level's par, and the hints taken if any.
pub(crate) struct StepLine {
    pub(crate) steps: u16,
    pub(crate) par: u8,
    pub(crate) hints: u16,
}

impl fmt::Display for StepLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (step, par, hint) = if chinese() {
            ("步数", "最少", "提示")
        } else {
            ("STEP", "PAR", "HINT")
        };
        write!(output, "{step} {:02} · {par} {:02}", self.steps, self.par)?;
        if self.hints != 0 {
            write!(output, " · {hint} {}", self.hints)?;
        }
        Ok(())
    }
}

/// A hint: the move to make, and how many moves the best route still needs.
pub(crate) struct HintText {
    pub(crate) direction: Direction4,
    pub(crate) remaining: u16,
}

impl fmt::Display for HintText {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let arrow = self.direction.arrow();
        if chinese() {
            write!(output, "提示 {arrow} · 最少还需 {} 步", self.remaining)
        } else {
            write!(output, "Hint {arrow} · {} moves to go", self.remaining)
        }
    }
}

/// A level tile of the expedition map: its number over its stars, or over
/// whether it is being played, open or locked.
pub(crate) struct LevelTile {
    pub(crate) level: u8,
    pub(crate) stars: u8,
    pub(crate) current: bool,
    pub(crate) locked: bool,
}

impl fmt::Display for LevelTile {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(output, "{:02}", self.level + 1)?;
        if self.locked {
            write!(output, "{}", tr!(en: "LOCKED", zh: "未开放"))
        } else if self.stars != 0 {
            write!(output, "{}", Stars(self.stars))
        } else if self.current {
            write!(output, "{}", tr!(en: "PLAY", zh: "进行中"))
        } else {
            write!(output, "{}", tr!(en: "OPEN", zh: "可挑战"))
        }
    }
}

/// The map's progress line: levels cleared and the chapter on show.
pub(crate) struct MapSummary {
    pub(crate) cleared: u8,
    pub(crate) chapter: u8,
}

impl fmt::Display for MapSummary {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (cleared, total, chapter) = (self.cleared, EXPEDITION_LEVELS, self.chapter + 1);
        if chinese() {
            write!(
                output,
                "已完成 {cleared:02} / {total} · 第 {chapter} / 6 章"
            )
        } else {
            write!(
                output,
                "CLEARED {cleared:02} / {total} · CHAPTER {chapter} / 6"
            )
        }
    }
}

/// A chapter by number and name, for its briefing.
pub(crate) struct ChapterTitle {
    pub(crate) chapter: u8,
    pub(crate) name: Phrase,
}

impl fmt::Display for ChapterTitle {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(output, "第 {} 章 · {}", self.chapter + 1, self.name)
        } else {
            write!(output, "CHAPTER {} · {}", self.chapter + 1, self.name)
        }
    }
}

/// One chapter of the final log: levels cleared and stars earned.
pub(crate) struct ChapterLine {
    pub(crate) progress: ExpeditionProgress,
    pub(crate) chapter: u8,
    pub(crate) name: Phrase,
}

impl fmt::Display for ChapterLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (cleared, stars) = self.progress.chapter(self.chapter);
        write!(
            output,
            "0{}  {}   {cleared}/6   {stars}/18 ★",
            self.chapter + 1,
            self.name
        )
    }
}

/// A level by number in the game's own word for it: "STATION 7", "第 7 站".
pub(crate) struct LevelNumber {
    pub(crate) level: u8,
    pub(crate) unit: Phrase,
}

impl fmt::Display for LevelNumber {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(output, "第 {} {}", self.level + 1, self.unit)
        } else {
            write!(output, "{} {}", self.unit, self.level + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn shared_labels_read_english_outside_a_world() {
        let tile = LevelTile {
            level: 6,
            stars: 2,
            current: false,
            locked: false,
        };
        assert_eq!(tile.to_string(), "07\n★★☆");
        let locked = LevelTile {
            locked: true,
            ..tile
        };
        assert_eq!(locked.to_string(), "07\nLOCKED");
        let steps = StepLine {
            steps: 3,
            par: 12,
            hints: 1,
        };
        assert_eq!(steps.to_string(), "STEP 03 · PAR 12 · HINT 1");
        let number = LevelNumber {
            level: 6,
            unit: tr!(en: "STATION", zh: "站"),
        };
        assert_eq!(number.to_string(), "STATION 7");
        let cleared = tr!(en: "DONE", zh: "完成");
        assert_eq!(
            result_title(ExpeditionModal::Result, 5, cleared).en,
            "CHAPTER CLEAR"
        );
        assert_eq!(result_title(ExpeditionModal::Result, 4, cleared), cleared);
        assert_eq!(
            result_next(ExpeditionModal::Final, tr!(en: "Next →", zh: "下一站 →")).zh,
            "查看档案 →"
        );
    }
}
