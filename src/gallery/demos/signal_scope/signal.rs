use super::state::{ScopeState, TriggerEdge};
use crate::prelude::Fixed;

pub(super) const UART_MESSAGE: &[u8] = b"HELLO WORLD";
pub(super) const UART_BITS_PER_SYMBOL: i32 = 10;
pub(super) const UART_SYMBOL_MS: u32 = 48;
pub(super) const UART_RISING_OFFSET: i32 = 9;
pub(super) const UART_FALLING_OFFSET: i32 = 0;
const PERSISTENCE_STEP_MS: u32 = 13;

pub(super) fn uart_bit(index: i32) -> bool {
    let bit_count = UART_MESSAGE.len() as i32 * UART_BITS_PER_SYMBOL;
    let wrapped = index.rem_euclid(bit_count);
    let byte = UART_MESSAGE[(wrapped / UART_BITS_PER_SYMBOL) as usize];
    match wrapped % UART_BITS_PER_SYMBOL {
        0 => false,
        1..=8 => byte & (1 << ((wrapped % UART_BITS_PER_SYMBOL) - 1)) != 0,
        _ => true,
    }
}

fn uart_level(index: i32) -> Fixed {
    if uart_bit(index) {
        Fixed::ONE
    } else {
        -Fixed::ONE
    }
}

pub(super) fn bandwidth_limited_step(distance: Fixed) -> Fixed {
    let transition_half = Fixed::from_ratio(1, 4);
    let mut response = if distance <= -transition_half {
        Fixed::ZERO
    } else if distance >= transition_half {
        Fixed::ONE
    } else {
        let t = (distance + transition_half) / (transition_half * Fixed::from_int(2));
        let t2 = t * t;
        let t3 = t2 * t;
        t3 * (t * (t * Fixed::from_int(6) - Fixed::from_int(15)) + Fixed::from_int(10))
    };

    if distance > transition_half && distance < Fixed::ONE {
        let ring_distance = distance - transition_half;
        let ring_span = Fixed::ONE - transition_half;
        let envelope = Fixed::ONE - ring_distance / ring_span;
        response += Fixed::sin_deg(ring_distance / ring_span * Fixed::from_int(900))
            * envelope
            * envelope
            * Fixed::from_ratio(9, 100);
    } else if distance < -transition_half && distance > -Fixed::from_ratio(3, 4) {
        let ring_distance = -distance - transition_half;
        let ring_span = Fixed::from_ratio(1, 2);
        let envelope = Fixed::ONE - ring_distance / ring_span;
        response -= Fixed::sin_deg(ring_distance / ring_span * Fixed::from_int(540))
            * envelope
            * envelope
            * Fixed::from_ratio(3, 100);
    }
    response
}

impl ScopeState {
    pub(super) fn base_angle(self, channel: u8, unit_x: Fixed) -> Fixed {
        let cycles = Fixed::from_int(i32::from(self.time_scale + 4));
        if channel == 0 {
            let gain = Fixed::from_ratio(i32::from(self.gain_a + 1), 3);
            let level = (self.trigger_level / gain)
                .clamp(Fixed::from_ratio(-9, 10), Fixed::from_ratio(9, 10));
            let cosine = (Fixed::ONE - level * level).sqrt();
            let signed_cosine = match self.trigger_edge {
                TriggerEdge::Rising => cosine,
                TriggerEdge::Falling => -cosine,
            };
            let trigger_angle =
                Fixed::atan2(level, signed_cosine) * Fixed::from_int(180) / Fixed::PI;
            (unit_x - Fixed::from_ratio(1, 2)) * Fixed::from_int(360) * cycles + trigger_angle
        } else {
            unit_x * Fixed::from_int(360) * cycles + Fixed::from_int(74)
        }
    }

    pub(super) fn sample_at_angle(self, channel: u8, angle: Fixed) -> Fixed {
        let sample = match channel {
            1 => {
                if Fixed::sin_deg(angle) >= Fixed::ZERO {
                    Fixed::from_ratio(4, 5)
                } else {
                    Fixed::from_ratio(-4, 5)
                }
            }
            _ => {
                let reference = self.base_angle(0, Fixed::from_ratio(1, 2));
                let second = Fixed::sin_deg(angle * Fixed::from_int(2) + Fixed::from_int(31))
                    - Fixed::sin_deg(reference * Fixed::from_int(2) + Fixed::from_int(31));
                let third = Fixed::sin_deg(angle * Fixed::from_int(3) - Fixed::from_int(19))
                    - Fixed::sin_deg(reference * Fixed::from_int(3) - Fixed::from_int(19));
                let interference =
                    Fixed::sin_deg(angle * Fixed::from_ratio(7, 5) + Fixed::from_int(13))
                        - Fixed::sin_deg(reference * Fixed::from_ratio(7, 5) + Fixed::from_int(13));
                Fixed::sin_deg(angle)
                    + second * Fixed::from_ratio(11, 100)
                    + third * Fixed::from_ratio(6, 100)
                    + interference * Fixed::from_ratio(7, 100)
            }
        };
        let gain = if channel == 0 { self.gain_a } else { 1 };
        sample * Fixed::from_ratio(i32::from(gain + 1), 3)
    }

    fn trace_time(self, history: u8) -> Fixed {
        const PERIOD_MS: u32 = 3_600;
        let history_ms = u32::from(history) * PERSISTENCE_STEP_MS;
        let now = self.trace_clock_ms % PERIOD_MS;
        Fixed::from_int(((now + PERIOD_MS - history_ms % PERIOD_MS) % PERIOD_MS) as i32)
    }

    pub(super) fn trace_sample(self, channel: u8, unit_x: Fixed, history: u8) -> Fixed {
        let time = self.trace_time(history);
        let spatial = unit_x * Fixed::from_int(if channel == 0 { 1_440 } else { 2_160 });
        let jitter_angle = Fixed::sin_deg(time * Fixed::from_int(7) + spatial)
            * if channel == 0 {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::from_int(5)
            };
        let angle = self.base_angle(channel, unit_x) + jitter_angle;
        let mut sample = self.sample_at_angle(channel, angle);
        if channel == 0 {
            let center = Fixed::from_ratio(1, 2);
            let shape_phase = time * Fixed::from_int(2);
            let local_modulation = Fixed::sin_deg(spatial + shape_phase)
                - Fixed::sin_deg(center * Fixed::from_int(1_440) + shape_phase);
            let wander_phase = time / Fixed::from_int(4);
            let baseline_wander = Fixed::sin_deg(unit_x * Fixed::from_int(270) + wander_phase)
                - Fixed::sin_deg(center * Fixed::from_int(270) + wander_phase);
            sample += sample * local_modulation * Fixed::from_ratio(5, 100)
                + baseline_wander * Fixed::from_ratio(3, 100);
            sample += Fixed::sin_deg(angle * Fixed::from_int(11) + time * Fixed::from_int(13))
                * Fixed::from_ratio(2, 100);
        }
        sample
    }

    pub(super) fn sweep_jitter(self, channel: u8, history: u8) -> Fixed {
        let time = self.trace_time(history);
        Fixed::sin_deg(time * Fixed::from_int(5) + Fixed::from_int(i32::from(channel) * 97))
            * Fixed::from_ratio(3, 4)
    }

    pub(super) fn uart_symbol_index(self, history: u8) -> usize {
        let history_ms = u32::from(history) * PERSISTENCE_STEP_MS;
        let acquisition_ms = self.trace_clock_ms.saturating_sub(history_ms);
        (acquisition_ms / UART_SYMBOL_MS) as usize % UART_MESSAGE.len()
    }

    pub(super) fn uart_anchor(self, history: u8) -> i32 {
        let symbol = self.uart_symbol_index(history) as i32;
        symbol * UART_BITS_PER_SYMBOL
            + match self.trigger_edge {
                TriggerEdge::Rising => UART_RISING_OFFSET,
                TriggerEdge::Falling => UART_FALLING_OFFSET,
            }
    }

    pub(super) fn uart_label(self) -> &'static str {
        match self.uart_symbol_index(0) {
            0 => "UART · H · 01001000",
            1 => "UART · E · 01000101",
            2 | 3 | 9 => "UART · L · 01001100",
            4 | 7 => "UART · O · 01001111",
            5 => "UART · SPACE · 00100000",
            6 => "UART · W · 01010111",
            8 => "UART · R · 01010010",
            _ => "UART · D · 01000100",
        }
    }

    pub(super) fn uart_wave_sample(self, relative_bits: Fixed, history: u8) -> Fixed {
        let time = self.trace_time(history);
        let local_jitter =
            (Fixed::sin_deg(time * Fixed::from_int(9) + relative_bits * Fixed::from_int(83))
                - Fixed::sin_deg(time * Fixed::from_int(9)))
                * Fixed::from_ratio(3, 50);
        let position = relative_bits + local_jitter;
        let anchor = self.uart_anchor(history);
        let first_bit = position.floor().to_int() - 3;
        let mut signal = uart_level(anchor + first_bit);
        for boundary in first_bit + 1..=first_bit + 7 {
            let before = uart_level(anchor + boundary - 1);
            let after = uart_level(anchor + boundary);
            if before != after {
                signal +=
                    (after - before) * bandwidth_limited_step(position - Fixed::from_int(boundary));
            }
        }
        let noise = Fixed::sin_deg(position * Fixed::from_int(1_937) + time * Fixed::from_int(29))
            * Fixed::from_ratio(1, 100);
        (signal + noise).clamp(Fixed::from_ratio(-6, 5), Fixed::from_ratio(6, 5))
            * Fixed::from_ratio(3, 5)
    }
}
