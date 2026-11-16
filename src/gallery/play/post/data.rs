use crate::core::i18n::Phrase;

/// One shift of the day: the parcels it sends, how often, and how many may be
/// on the rails at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shift {
    pub(crate) name: Phrase,
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
        name: crate::tr!(en: "Dawn", zh: "晨班"),
        parcels: &[0, 0, 1, 1, 2, 2],
        spawn_period: 360,
        in_flight: 2,
    },
    Shift {
        name: crate::tr!(en: "Morning", zh: "上午班"),
        parcels: &[0, 1, 2, 0, 2, 1, 0, 1, 2],
        spawn_period: 276,
        in_flight: 3,
    },
    Shift {
        name: crate::tr!(en: "Midday", zh: "午班"),
        parcels: &[2, 0, 1, 2, 1, 0, 1, 2, 0, 2, 1, 0],
        spawn_period: 240,
        in_flight: 3,
    },
    Shift {
        name: crate::tr!(en: "Afternoon", zh: "下午班"),
        parcels: &[1, 2, 0, 2, 1, 0, 2, 0, 1, 2, 0, 1],
        spawn_period: 204,
        in_flight: 4,
    },
    Shift {
        name: crate::tr!(en: "Overtime", zh: "加班"),
        parcels: &[2, 1, 2, 0, 1, 0, 2, 1, 2, 0, 1, 0, 2, 1, 0],
        spawn_period: 180,
        in_flight: 4,
    },
    Shift {
        name: crate::tr!(en: "Night", zh: "夜班"),
        parcels: &[0, 2, 1, 2, 0, 1, 1, 2, 0, 2, 1, 0, 2, 0, 1, 2, 1, 0],
        spawn_period: 156,
        in_flight: 5,
    },
];

/// The towns at the ends of the rails, A to C.
pub(crate) const STATIONS: [Phrase; 3] = [
    crate::tr!(en: "Mint Port", zh: "薄荷港"),
    crate::tr!(en: "Wisteria", zh: "紫藤站"),
    crate::tr!(en: "Sunset Bay", zh: "日落湾"),
];

pub(super) const STATION_FLASH_TICKS: u8 = 48;
