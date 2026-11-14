/// One shift of the day: the parcels it sends, how often, and how many may be
/// on the rails at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shift {
    pub(crate) name: &'static str,
    /// Station of each parcel in dispatch order: 0 = A, 1 = B, 2 = C.
    pub(crate) parcels: &'static [u8],
    /// Half ticks between dispatches at 1× speed; 120 half ticks are a second.
    pub(crate) spawn_period: u16,
    /// Parcels allowed on the rails at once.
    pub(crate) in_flight: u8,
}

pub(crate) const SHIFT_COUNT: usize = 6;

/// The day ramps up: more parcels, sent faster, more of them in flight.
pub(crate) const SHIFTS: [Shift; SHIFT_COUNT] = [
    Shift {
        name: "晨班",
        parcels: &[0, 0, 1, 1, 2, 2],
        spawn_period: 360,
        in_flight: 2,
    },
    Shift {
        name: "上午班",
        parcels: &[0, 1, 2, 0, 2, 1, 0, 1, 2],
        spawn_period: 276,
        in_flight: 3,
    },
    Shift {
        name: "午班",
        parcels: &[2, 0, 1, 2, 1, 0, 1, 2, 0, 2, 1, 0],
        spawn_period: 240,
        in_flight: 3,
    },
    Shift {
        name: "下午班",
        parcels: &[1, 2, 0, 2, 1, 0, 2, 0, 1, 2, 0, 1],
        spawn_period: 204,
        in_flight: 4,
    },
    Shift {
        name: "加班",
        parcels: &[2, 1, 2, 0, 1, 0, 2, 1, 2, 0, 1, 0, 2, 1, 0],
        spawn_period: 180,
        in_flight: 4,
    },
    Shift {
        name: "夜班",
        parcels: &[0, 2, 1, 2, 0, 1, 1, 2, 0, 2, 1, 0, 2, 0, 1, 2, 1, 0],
        spawn_period: 156,
        in_flight: 5,
    },
];

pub(super) const STATION_FLASH_TICKS: u8 = 48;
