# mirx

MIRX is the ahead-of-time binary image format used by [mirui](https://github.com/W-Mai/mirui).
It bundles pixel data (and optionally palette / alpha plane / metadata) in a
form that mirui can `include_bytes!` and read with zero copies on embedded
targets, while CHUNK mode also carries multiple frames, animations, or vector
content.

`no_std + alloc` with no external dependencies. Consumed by mirui directly
and by host-side conversion tools such as [`icu`](https://github.com/W-Mai/icu).

## License

MIT.
