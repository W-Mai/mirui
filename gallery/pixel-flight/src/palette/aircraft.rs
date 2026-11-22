use super::opaque;

// MIRX material ranges start at 1, 17, 33 and 49. The engine glow keeps
// the same colors at every light level.
const fn lighting(ramps: [&[u32]; 4]) -> [[u32; 96]; 3] {
    let mut palettes = [[0; 96]; 3];
    let mut material = 0;
    while material < ramps.len() {
        let ramp = ramps[material];
        let first = 1 + material * 16;
        let mut shade = 0;
        while shade < ramp.len() {
            let bright = if material == 3 || shade + 1 == ramp.len() {
                shade
            } else {
                shade + 1
            };
            let dark = if material == 3 {
                shade
            } else {
                shade.saturating_sub(1)
            };
            palettes[0][first + shade] = opaque(ramp[shade]);
            palettes[1][first + shade] = opaque(ramp[bright]);
            palettes[2][first + shade] = opaque(ramp[dark]);
            shade += 1;
        }
        material += 1;
    }
    palettes
}

#[rustfmt::skip]
pub(super) static DAY: [[u32; 96]; 3] = lighting([
    &[0x0E0F16, 0x1D1F2A, 0x2C2F3C, 0x3D4150, 0x545969, 0x707686, 0x969CAA, 0xC4C9D3, 0xECEFF4], // Airframe
    &[0x5C1808, 0x962806, 0xCD3E06, 0xF0580E, 0xFF8028, 0xFFB060], // Orange markings
    &[0x042634, 0x06465A, 0x0E6A80, 0x1E8CA0, 0x54B2BE, 0xD6F0F0], // Canopy
    &[0x46160E, 0xB0300A, 0xF86E10, 0xFFB236, 0xFFECAA], // Engine glow
]);

#[rustfmt::skip]
pub(super) static SUNSET: [[u32; 96]; 3] = lighting([
    &[0x140F18, 0x231D29, 0x332B39, 0x443B4B, 0x5C5061, 0x786A7B, 0x9F8B9B, 0xCEB3BF, 0xF7D4DD], // Airframe
    &[0x60180F, 0x9A270F, 0xD03C12, 0xF4531A, 0xFF7530, 0xFF9F60], // Orange markings
    &[0x0B2231, 0x0F3E51, 0x185D72, 0x2A7B8E, 0x609CA9, 0xE2D5D9], // Canopy
    &[0x4B1613, 0xB32F14, 0xFD661C, 0xFFA03C, 0xFFD29F], // Engine glow
]);

#[rustfmt::skip]
pub(super) static NIGHT: [[u32; 96]; 3] = lighting([
    &[0x0C0E17, 0x191C2A, 0x262A3A, 0x353A4D, 0x4A5064, 0x62697E, 0x848BA0, 0xACB3C5, 0xCFD5E4], // Airframe
    &[0x51160A, 0x842408, 0xB43808, 0xD34F10, 0xE07228, 0xE09D5B], // Orange markings
    &[0x032233, 0x053F56, 0x0C5F79, 0x1A7D96, 0x4A9FB2, 0xBCD6E0], // Canopy
    &[0x3D1410, 0x9A2B0C, 0xDA6212, 0xE09F35, 0xE0D3A0], // Engine glow
]);
