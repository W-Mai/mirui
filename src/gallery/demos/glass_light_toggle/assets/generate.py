"""Bake reusable gradient stamps as straight-alpha RGBA8888 pixels."""

from math import hypot
from pathlib import Path

SIDE = 128
ROOT = Path(__file__).parent


def bake(name, stops, center, radius):
    pixels = bytearray()
    for y in range(SIDE):
        for x in range(SIDE):
            t = min(1, hypot(x / (SIDE - 1) - center[0], y / (SIDE - 1) - center[1]) / radius)
            for (a, ca), (b, cb) in zip(stops, stops[1:]):
                if t <= b:
                    f = max(0, (t - a) / (b - a))
                    pixels.extend(round(v + (w - v) * f) for v, w in zip(ca, cb))
                    break
    (ROOT / f'{name}.rgba').write_bytes(pixels)


bake('dark', [(0, (34, 34, 36, 255)), (.55, (15, 15, 17, 255)), (1, (8, 8, 9, 255))], (.8, 0), 1.2)
bake('light', [(0, (249, 242, 230, 255)), (.55, (230, 229, 225, 255)), (1, (219, 219, 217, 255))], (.7, 0), 1.2)
bake('blue', [(0, (146, 161, 255, 195)), (.24, (106, 126, 255, 123)), (.6, (89, 112, 255, 40)), (1, (88, 108, 255, 0))], (.5, .5), .5)
bake('amber', [(0, (255, 196, 87, 220)), (.24, (255, 174, 56, 150)), (.6, (255, 176, 65, 40)), (1, (255, 183, 80, 0))], (.5, .5), .5)
