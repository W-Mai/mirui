# MIRX sprite-stack format

## Sparse levels of detail

An optional `SLOD` chunk contains additional levels for one primary IMAGE and its SSTK descriptor. Coordinates use the original layer canvas. Each cell starts at a multiple of its level's step and covers up to that many base pixels on each axis, clipped to the edge of the canvas. Empty layers retain their original index. Indexed atlases keep their palette indices and RGBA atlases keep their original colors.

The little-endian header contains the four-byte magic, a u16 version (1), the color byte count (1 or 4), the reduction mode (0 for dominant, 1 for material ramps), then six u16 fields for extra level count, layer count, ramp count, canvas width, canvas height and reserved zero. Each ramp is an inclusive pair of palette indices. Levels appear in increasing step order. Each level table record stores its u16 step, a reserved zero u16, then a u32 payload offset and byte length relative to the start of SLOD. Level data follows contiguously in record order. Each payload begins with one u32 sample offset per layer and a final end offset, followed by row-major cells containing u16 x, u16 y and either a u8 palette index or four RGBA bytes.
