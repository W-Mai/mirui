use super::super::change::ChangeSet;
use super::model::MarbleModel;
use super::transport::MAX_SOUND_EVENTS;
use super::types::{
    MAX_BALLS, MAX_PADS, MAX_PARTICLES, MAX_RINGS, PAD_PITCHES, PALETTE, Pad, PadTimbre, THEMES,
    Vec2,
};
use crate::types::Fixed64;

pub(super) fn reset_scene(model: &mut MarbleModel, index: usize) -> ChangeSet {
    model.cancel_input();
    model.scene = index.min(THEMES.len() - 1);
    model.gravity = model.theme().gravity;
    model.bpm = [96, 72, 128][model.scene];
    model.pads = [None; MAX_PADS];
    model.balls = [None; MAX_BALLS];
    model.rings = [None; MAX_RINGS];
    model.particles = [None; MAX_PARTICLES];
    let poses = [
        (143, 108, 18),
        (334, 106, 19),
        (240, 161, 21),
        (107, 211, 18),
        (368, 205, 19),
    ];
    const PITCHES: [[u8; 5]; 3] = [
        [
            PAD_PITCHES[5],
            PAD_PITCHES[3],
            PAD_PITCHES[7],
            PAD_PITCHES[0],
            PAD_PITCHES[6],
        ],
        [
            PAD_PITCHES[0],
            PAD_PITCHES[3],
            PAD_PITCHES[2],
            PAD_PITCHES[0],
            PAD_PITCHES[1],
        ],
        [
            PAD_PITCHES[5],
            PAD_PITCHES[8],
            PAD_PITCHES[2],
            PAD_PITCHES[0],
            PAD_PITCHES[9],
        ],
    ];
    const TIMBRES: [[PadTimbre; 5]; 3] = [
        [
            PadTimbre::Mallet,
            PadTimbre::Mallet,
            PadTimbre::Mallet,
            PadTimbre::Drum,
            PadTimbre::Mallet,
        ],
        [
            PadTimbre::Synth,
            PadTimbre::Synth,
            PadTimbre::Mallet,
            PadTimbre::Bass,
            PadTimbre::Synth,
        ],
        [
            PadTimbre::Synth,
            PadTimbre::Mallet,
            PadTimbre::Bass,
            PadTimbre::Drum,
            PadTimbre::Synth,
        ],
    ];
    for (slot, (x, y, radius)) in poses.into_iter().enumerate() {
        let color = PALETTE[if model.scene == 1 {
            (slot + 1) % 5
        } else {
            slot
        }];
        model.pads[slot] = Some(Pad {
            id: slot as u32 + 1,
            letter: b'A' + slot as u8,
            pos: Vec2::new(x, y),
            radius: Fixed64::from_int(radius),
            color,
            bounce: Fixed64::from_ratio(11, 10),
            pulse: Fixed64::ZERO,
            pitch: PITCHES[model.scene][slot],
            timbre: TIMBRES[model.scene][slot],
            last_hit: Fixed64::from_int(-99),
        });
    }
    model.selected = 0;
    model.next_id = 6;
    model.hits = 0;
    model.sim_time = Fixed64::ZERO;
    model.flash = Fixed64::ZERO;
    model.paused = false;
    model.tilt = Vec2::default();
    model.target = Vec2::default();
    model.keys = 0;
    model.ring_cursor = 0;
    model.particle_cursor = 0;
    model.sounds = [None; MAX_SOUND_EVENTS];
    model.sound_len = 0;
    model.stop_recording();
    model.transport_steps = Fixed64::ZERO;
    model.last_audio_step = 0;
    for (x, y, vx, vy) in [
        (74, 83, 82, 25),
        (221, 115, 66, 21),
        (406, 87, -83, 32),
        (48, 196, 83, -60),
        (300, 183, -60, -79),
    ] {
        let _ = model.spawn_at(Vec2::new(x, y), Vec2::new(vx, vy));
    }
    model.page = super::types::Page::Play;
    model.inspector = false;
    model.add_mode = false;
    model.notify("SCENE LOADED / EDITS RESET");
    ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
}
