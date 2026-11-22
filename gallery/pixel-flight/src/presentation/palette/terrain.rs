use super::channels;

// Material order is shared with the procedural height field.
#[rustfmt::skip]
pub const TOP: [[u8; 3]; 17] = swatches([
    0x2468B4, 0x2C7CC6, 0x42A0D8, 0x68CCD6, // Water, deep to shallow
    0xEEDCA4, 0xDEC88C,                     // Sand
    0x60B234, 0x46942C, 0x86CE4C,           // Grass
    0x469E40, 0x2A7A36, 0x1C5C2E,           // Trees
    0x928C86, 0x76706E, 0xB2ACA2,           // Rock
    0xF6FAFF, 0xD6E2F4,                     // Snow
]);

#[rustfmt::skip]
pub const WALL: [[u8; 3]; 17] = swatches([
    0x2468B4, 0x2C7CC6, 0x42A0D8, 0x68CCD6, // Water, deep to shallow
    0xCCB47A, 0xCCB47A,                     // Sand
    0x38782C, 0x326C2A, 0x3C802E,           // Grass
    0x184E2A, 0x164828, 0x144026,           // Trees
    0x605A5C, 0x565256, 0x686262,           // Rock
    0xBACAE4, 0xB0C0DC,                     // Snow
]);

const fn swatches<const N: usize>(rgb: [u32; N]) -> [[u8; 3]; N] {
    let mut colors = [[0; 3]; N];
    let mut index = 0;
    while index < N {
        colors[index] = channels(rgb[index]);
        index += 1;
    }
    colors
}
