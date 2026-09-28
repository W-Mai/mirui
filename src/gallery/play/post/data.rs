pub(crate) const MANIFEST_COUNT: usize = 3;

const MANIFEST_0: [u8; 9] = [0, 1, 2, 0, 2, 1, 0, 1, 2];
const MANIFEST_1: [u8; 12] = [2, 0, 1, 2, 1, 0, 1, 2, 0, 2, 1, 0];
const MANIFEST_2: [u8; 6] = [0, 0, 1, 1, 2, 2];
pub(super) const MANIFESTS: [&[u8]; MANIFEST_COUNT] = [&MANIFEST_0, &MANIFEST_1, &MANIFEST_2];
pub(super) const SPAWN_PERIOD_HALF_TICKS: u16 = 276;
pub(super) const STATION_FLASH_TICKS: u8 = 48;
