//! The saved layout: the scene, its settings and every pad. Marbles, a
//! recorded loop and the hit count start fresh.

use super::model::MarbleModel;
use super::presets;
use super::types::{MAX_PADS, PAD_PITCHES, PALETTE, Pad, PadTimbre, THEMES, Vec2};
use crate::gallery::play::expeditions::{ExpeditionSaveError, finish_packet, validate_packet};
use crate::types::Fixed64;

const SAVE_MAGIC: [u8; 4] = *b"MRB1";
const SAVE_VERSION: u8 = 1;
/// Letter, x and y in sixteenths of a unit, radius, colour, bounce, pitch
/// and sound.
const PAD_BYTES: usize = 10;
/// Magic, version, scene, gravity, tempo and flags, then six pad records.
const SAVE_PAYLOAD: usize = 9 + MAX_PADS * PAD_BYTES;
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;
const TIMBRES: [PadTimbre; 4] = [
    PadTimbre::Mallet,
    PadTimbre::Synth,
    PadTimbre::Bass,
    PadTimbre::Drum,
];

/// A layout read back from a save, checked and ready to apply.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MarbleLayout {
    scene: u8,
    gravity: Fixed64,
    bpm: u16,
    trails: bool,
    feedback: bool,
    pads: [Option<Pad>; MAX_PADS],
}

fn hundredths(value: Fixed64) -> i64 {
    (value * Fixed64::from_int(100) + Fixed64::from_ratio(1, 2)).to_int()
}

fn sixteenths(value: Fixed64) -> i16 {
    (value * Fixed64::from_int(16) + Fixed64::from_ratio(1, 2))
        .to_int()
        .clamp(0, i64::from(i16::MAX)) as i16
}

impl MarbleLayout {
    pub(crate) fn decode(input: &[u8]) -> Result<Self, ExpeditionSaveError> {
        validate_packet(input, SAVE_PAYLOAD)?;
        if input[..4] != SAVE_MAGIC {
            return Err(ExpeditionSaveError::InvalidMagic);
        }
        if input[4] != SAVE_VERSION {
            return Err(ExpeditionSaveError::UnsupportedVersion);
        }
        let (scene, gravity, bpm, flags) = (input[5], input[6], input[7], input[8]);
        if usize::from(scene) >= THEMES.len()
            || gravity > 160
            || !(55..=160).contains(&bpm)
            || flags > 0b11
        {
            return Err(ExpeditionSaveError::InvalidState);
        }
        let mut pads = [None; MAX_PADS];
        for (slot, record) in input[9..SAVE_PAYLOAD].chunks_exact(PAD_BYTES).enumerate() {
            pads[slot] = decode_pad(slot, record)?;
        }
        let letters = pads.iter().flatten().map(|pad| pad.letter);
        let mut seen = 0_u8;
        for letter in letters {
            let bit = 1 << (letter - b'A');
            if seen & bit != 0 {
                return Err(ExpeditionSaveError::InvalidRecord);
            }
            seen |= bit;
        }
        if seen == 0 {
            return Err(ExpeditionSaveError::InvalidRecord);
        }
        Ok(Self {
            scene,
            gravity: Fixed64::from_ratio(i64::from(gravity), 100),
            bpm: u16::from(bpm),
            trails: flags & 1 != 0,
            feedback: flags & 2 != 0,
            pads,
        })
    }
}

fn decode_pad(slot: usize, record: &[u8]) -> Result<Option<Pad>, ExpeditionSaveError> {
    if record[0] == 0 {
        return Ok(None);
    }
    let letter = record[0];
    let x = i16::from_le_bytes([record[1], record[2]]);
    let y = i16::from_le_bytes([record[3], record[4]]);
    let (radius, color, bounce, pitch, timbre) =
        (record[5], record[6], record[7], record[8], record[9]);
    if !(b'A'..=b'F').contains(&letter)
        || x < 0
        || y < 0
        || !(13..=23).contains(&radius)
        || usize::from(color) >= PALETTE.len()
        || !(70..=140).contains(&bounce)
        || usize::from(pitch) >= PAD_PITCHES.len()
        || usize::from(timbre) >= TIMBRES.len()
    {
        return Err(ExpeditionSaveError::InvalidRecord);
    }
    let mut pad = Pad {
        id: slot as u32 + 1,
        letter,
        pos: Vec2::default(),
        radius: Fixed64::from_int(i64::from(radius)),
        color: PALETTE[usize::from(color)],
        bounce: Fixed64::from_ratio(i64::from(bounce), 100),
        pulse: Fixed64::ZERO,
        pitch: PAD_PITCHES[usize::from(pitch)],
        timbre: TIMBRES[usize::from(timbre)],
        last_hit: Fixed64::from_int(-99),
    };
    pad.pos = MarbleModel::bounded(
        &pad,
        Vec2 {
            x: Fixed64::from_ratio(i64::from(x), 16),
            y: Fixed64::from_ratio(i64::from(y), 16),
        },
    );
    Ok(Some(pad))
}

impl MarbleModel {
    pub(crate) fn encode_into(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.scene as u8;
        output[6] = hundredths(self.gravity).clamp(0, 160) as u8;
        output[7] = self.bpm.clamp(55, 160) as u8;
        output[8] = u8::from(self.trails) | u8::from(self.feedback) << 1;
        let records = output[9..SAVE_PAYLOAD].chunks_exact_mut(PAD_BYTES);
        for (record, pad) in records.zip(self.pads) {
            record.fill(0);
            let Some(pad) = pad else {
                continue;
            };
            record[0] = pad.letter;
            record[1..3].copy_from_slice(&sixteenths(pad.pos.x).to_le_bytes());
            record[3..5].copy_from_slice(&sixteenths(pad.pos.y).to_le_bytes());
            record[5] = pad.radius.to_int().clamp(13, 23) as u8;
            record[6] = PALETTE
                .iter()
                .position(|color| *color == pad.color)
                .unwrap_or(0) as u8;
            record[7] = hundredths(pad.bounce).clamp(70, 140) as u8;
            record[8] = PAD_PITCHES
                .iter()
                .position(|pitch| *pitch == pad.pitch)
                .unwrap_or(0) as u8;
            record[9] = TIMBRES
                .iter()
                .position(|timbre| *timbre == pad.timbre)
                .unwrap_or(0) as u8;
        }
        finish_packet(output, SAVE_PAYLOAD);
        Ok(SAVE_LEN)
    }

    /// Opens the saved scene with its pads and settings and fresh marbles.
    pub(crate) fn restore(&mut self, layout: MarbleLayout) {
        let _ = presets::reset_scene(self, usize::from(layout.scene));
        self.gravity = layout.gravity;
        self.bpm = layout.bpm;
        self.trails = layout.trails;
        self.feedback = layout.feedback;
        self.pads = layout.pads;
        self.selected = self
            .pads
            .iter()
            .position(Option::is_some)
            .expect("a decoded layout has a pad");
        self.next_id = MAX_PADS as u32 + 1;
        self.toast = super::model::NO_TOAST;
        self.toast_left = Fixed64::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::play::marble::Page;

    fn saved(model: &MarbleModel) -> [u8; SAVE_LEN] {
        let mut bytes = [0; SAVE_LEN];
        assert_eq!(model.encode_into(&mut bytes), Ok(SAVE_LEN));
        bytes
    }

    #[test]
    fn a_layout_comes_back_with_its_pads_and_settings() {
        let mut model = MarbleModel::new();
        let _ = model.reset_scene(2);
        let _ = model.set_page(Page::Edit);
        let pad = model.selected_pad().pos;
        let _ = model.begin_board_drag(pad);
        let _ = model.move_board_drag(Vec2 {
            x: pad.x + Fixed64::from_ratio(41, 2),
            y: pad.y,
        });
        let _ = model.end_board_drag(false);
        let _ = model.remove_selected();
        let _ = model.set_bpm(Fixed64::from_int(140));
        let _ = model.set_bounce(Fixed64::from_ratio(13, 10));
        let _ = model.cycle_timbre();
        let _ = model.toggle_trails();
        let bytes = saved(&model);

        let mut restored = MarbleModel::new();
        restored.restore(MarbleLayout::decode(&bytes).unwrap());
        assert_eq!(restored.scene, 2);
        assert_eq!(restored.gravity, model.gravity);
        assert_eq!(restored.bpm, 140);
        assert!(!restored.trails);
        assert_eq!(restored.pad_count(), model.pad_count());
        for (left, right) in restored.pads.iter().zip(model.pads.iter()) {
            match (left, right) {
                (Some(left), Some(right)) => {
                    assert_eq!(left.letter, right.letter);
                    assert_eq!(left.pos, right.pos);
                    assert_eq!(left.radius, right.radius);
                    assert_eq!(left.bounce, right.bounce);
                    assert_eq!(left.pitch, right.pitch);
                    assert_eq!(left.timbre, right.timbre);
                    assert_eq!(left.color, right.color);
                }
                (None, None) => {}
                _ => panic!("pad slots differ"),
            }
        }
        assert_eq!(restored.ball_count(), 5);
        assert_eq!(saved(&restored), bytes);
    }

    #[test]
    fn a_layout_without_pads_or_with_twin_letters_is_refused() {
        let model = MarbleModel::new();
        let bytes = saved(&model);
        let mut empty = bytes;
        for slot in 0..MAX_PADS {
            empty[9 + slot * PAD_BYTES] = 0;
        }
        finish_packet(&mut empty, SAVE_PAYLOAD);
        assert!(MarbleLayout::decode(&empty).is_err());
        let mut twins = bytes;
        twins[9 + PAD_BYTES] = twins[9];
        finish_packet(&mut twins, SAVE_PAYLOAD);
        assert!(MarbleLayout::decode(&twins).is_err());
        let mut broken = bytes;
        broken[10] ^= 1;
        assert!(MarbleLayout::decode(&broken).is_err(), "checksum");
    }
}
