#!/usr/bin/env python3
"""Export the layered wordmark and assemble the README's SVG artwork."""

import argparse
import difflib
import json
import re
import tempfile
import xml.etree.ElementTree as ET
from collections import Counter
from hashlib import sha256
from pathlib import Path

SVG = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG)
ROOT = Path(__file__).resolve().parents[3]


def number(value):
    return f"{value:.5f}".rstrip("0").rstrip(".") if value else "0"


def align(frames):
    """Index-align draw lists whose elements appear and disappear.

    The renderer culls a draw that leaves its clip, so a frame can lack some of
    the elements another frame has. The longest frame is the reference: every
    frame is matched to it in order by draw kind, and a draw the frame lacks
    becomes `None`. The long unchanged runs anchor the match.
    """
    reference = max(frames, key=len)
    kinds = [op["tag"] for op in reference]
    aligned = []
    for frame in frames:
        row = [None] * len(reference)
        matcher = difflib.SequenceMatcher(
            None, kinds, [op["tag"] for op in frame], autojunk=False
        )
        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag == "delete":
                continue
            if i2 - i1 != j2 - j1:
                raise ValueError(
                    f"a frame draws elements the longest frame lacks: {tag}"
                )
            row[i1:i2] = frame[j1:j2]
        aligned.append(row)
    return aligned


def held(samples):
    """Fill frames that lack a draw with its nearest drawn sample."""
    first = next(sample for sample in samples if sample is not None)
    result = []
    for sample in samples:
        first = sample if sample is not None else first
        result.append(first)
    return result


def matrix(values):
    return "matrix(" + ",".join(number(value) for value in values) + ")"


def attribute(value):
    if isinstance(value, (float, int)):
        return number(value)
    return str(value)


def compact(path):
    """Round path coordinates to 1/100 px; captures are drawn at or below 1:1."""
    return re.sub(
        r"-?\d+\.\d+",
        lambda match: f"{float(match.group()):.2f}".rstrip("0").rstrip("."),
        path,
    )


def simplify(values, tolerance):
    keep = {0, len(values) - 1}
    pending = [(0, len(values) - 1)]
    while pending:
        first, last = pending.pop()
        worst, error = first, 0
        for index in range(first + 1, last):
            t = (index - first) / (last - first)
            deviation = max(
                abs(value - (a + (b - a) * t)) / bound
                for value, a, b, bound in zip(
                    values[index], values[first], values[last], tolerance
                )
            )
            if deviation > error:
                worst, error = index, deviation
        if error > 1:
            keep.add(worst)
            pending.extend([(first, worst), (worst, last)])
    return sorted(keep)


WIDTH, HEIGHT = 1280, 640
MONO = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace"
# Reduced-motion poster: aperture reopening with the saved badge still shown.
POSTER_SECONDS = 5.2
PALETTES = {
    "dark": {
        "dark": True,
        "bg": "#0e0f11",
        "ink": "#ececee",
        "quiet": "#8e9198",
        "faint": "#26282c",
        "edge": "#2e3035",
        "chip": "#16171a",
        "key": "#ffffff",
        "key_alpha": 0.035,
        "frame": "#151619",
        "frame_edge": "#393b40",
        "window": "#18191c",
        "dots": "#3c3e43",
        "address": "#0f1012",
        "strap": "#1a1b1e",
        "case_hi": "#9a9da3",
        "shadow": 0.55,
    },
    "light": {
        "dark": False,
        "bg": "#eeeff1",
        "ink": "#1e2024",
        "quiet": "#6b7079",
        "faint": "#d2d5da",
        "edge": "#d8dbe0",
        "chip": "#f8f8f9",
        "key": "#ffffff",
        "key_alpha": 0.5,
        "frame": "#f7f7f8",
        "frame_edge": "#cdd0d5",
        "window": "#ffffff",
        "dots": "#d6d9dd",
        "address": "#eeeff1",
        "strap": "#cdd0d5",
        "case_hi": "#ffffff",
        "shadow": 0.2,
    },
}


class Hero:
    def __init__(self, capture, theme):
        self.capture = capture
        self.theme = theme
        self.styles = []
        self.motions = {}
        self.clips = {}
        self.defs = ET.Element("defs")
        self.fps = capture["fps"]
        self.count = len(capture["screens"][0]["frames"])
        self.duration = self.count / self.fps
        self.poster = min(self.count - 1, round(POSTER_SECONDS * self.fps))
        self.stats = Counter()

    def resources(self):
        import numpy as np
        import vtracer
        from PIL import Image

        with tempfile.TemporaryDirectory(prefix="mirui-glyph-") as directory:
            directory = Path(directory)
            for index, item in enumerate(self.capture["definitions"]):
                kind = item["kind"]
                if kind == "glyph":
                    group = ET.SubElement(self.defs, "g", {"id": f"d{index}"})
                    width, height = item["width"], item["height"]
                    if not width or not height:
                        continue
                    pixels = np.asarray(item["samples"], dtype=np.uint8).reshape(
                        height, width
                    )
                    if item["distance"]:
                        mask = np.where(pixels >= 128, 0, 255).astype(np.uint8)
                        image = directory / f"{index}.png"
                        vector = directory / f"{index}.svg"
                        Image.fromarray(mask).save(image)
                        vtracer.convert_image_to_svg_py(
                            str(image),
                            str(vector),
                            colormode="binary",
                            mode="spline",
                            filter_speckle=1,
                            corner_threshold=90,
                            length_threshold=3.0,
                            path_precision=3,
                        )
                        for path in (
                            ET.parse(vector).getroot().findall(f"{{{SVG}}}path")
                        ):
                            path.attrib.pop("fill", None)
                            path.tag = "path"
                            group.append(path)
                    else:
                        runs_by_alpha = {}
                        for y, row in enumerate(pixels.tolist()):
                            x = 0
                            while x < width:
                                start, alpha = x, row[x]
                                x += 1
                                while x < width and row[x] == alpha:
                                    x += 1
                                if alpha:
                                    runs_by_alpha.setdefault(alpha, []).append(
                                        f"M{start} {y}h{x - start}v1h-{x - start}Z"
                                    )
                        for alpha, runs in runs_by_alpha.items():
                            ET.SubElement(
                                group,
                                "path",
                                {"d": "".join(runs), "opacity": number(alpha / 255)},
                            )
                elif kind == "text":
                    group = ET.SubElement(self.defs, "g", {"id": f"d{index}"})
                    for glyph in item["glyphs"]:
                        ET.SubElement(
                            group,
                            "use",
                            {
                                "href": f"#d{glyph['ref']}",
                                "transform": f"translate({number(glyph['x'])} {number(glyph['y'])}) scale({number(glyph['scale'])})",
                            },
                        )
                elif kind in ["radialGradient", "linearGradient"]:
                    attrs = {
                        key: attribute(value)
                        for key, value in item.items()
                        if key not in ["kind", "stops", "units", "spread", "transform"]
                    }
                    attrs.update(
                        id=f"d{index}",
                        gradientUnits=item["units"],
                        spreadMethod=item["spread"],
                        gradientTransform=matrix(item["transform"]),
                    )
                    gradient = ET.SubElement(self.defs, kind, attrs)
                    for offset, color, alpha in item["stops"]:
                        ET.SubElement(
                            gradient,
                            "stop",
                            {
                                "offset": number(offset),
                                "stop-color": color,
                                "stop-opacity": number(alpha),
                            },
                        )
                else:
                    raise ValueError(f"Unsupported captured resource: {kind}")

    def clip(self, rect):
        key = tuple(rect)
        if key not in self.clips:
            name = f"clip{len(self.clips)}"
            clip = ET.SubElement(
                self.defs, "clipPath", {"id": name, "clipPathUnits": "userSpaceOnUse"}
            )
            ET.SubElement(
                clip,
                "rect",
                {
                    key: number(value)
                    for key, value in zip(["x", "y", "width", "height"], rect)
                },
            )
            self.clips[key] = name
        return self.clips[key]

    def name(self, prefix):
        name = f"{prefix}{len(self.clips)}"
        self.clips[(prefix, name)] = name
        return name

    def keyed(self, element, key, values, indices, mode):
        """Loop `key` through the per-frame `values`, keeping only `indices`."""
        ET.SubElement(
            element,
            "animate",
            {
                "attributeName": key,
                "dur": f"{number(self.duration)}s",
                "repeatCount": "indefinite",
                "calcMode": mode,
                "keyTimes": ";".join(number(index / self.count) for index in indices)
                + ";1",
                "values": ";".join(attribute(values[index]) for index in indices)
                + ";"
                + attribute(values[0]),
            },
        )

    def numbers(self, element, key, values, tolerance):
        self.keyed(
            element, key, values, simplify([[v] for v in values], [tolerance]), "linear"
        )

    def steps(self, element, key, values, mode="discrete"):
        indices = [0] + [
            index for index in range(1, len(values)) if values[index] != values[index - 1]
        ]
        self.keyed(element, key, values, indices, mode)

    def rect_clip(self, rects):
        if all(rect == rects[0] for rect in rects):
            return self.clip(rects[0])
        # The slider fill reveals itself through a moving clip.
        name = self.name("clip")
        clip = ET.SubElement(
            self.defs, "clipPath", {"id": name, "clipPathUnits": "userSpaceOnUse"}
        )
        shape = ET.SubElement(
            clip,
            "rect",
            {
                key: number(value)
                for key, value in zip(["x", "y", "width", "height"], rects[0])
            },
        )
        for axis, key in enumerate(["x", "y", "width", "height"]):
            values = [rect[axis] for rect in rects]
            if any(value != values[0] for value in values):
                self.numbers(shape, key, values, 0.08)
        self.stats["animated_clips"] += 1
        return name

    def element(self, samples, ref):
        first = samples[0]
        attrs = {
            key: value
            for key, value in first.items()
            if key not in ["tag", "clip", "ref"]
        }
        if ref is not None:
            attrs["href"] = f"#d{ref}"
        attrs["transform"] = matrix(first["transform"])
        if "d" in attrs:
            attrs["d"] = compact(attrs["d"])
        element = ET.Element(
            first["tag"], {key: attribute(value) for key, value in attrs.items()}
        )
        if len(samples) == 1:
            return element
        for key in sorted(first.keys() - {"tag", "clip", "ref"}):
            values = [sample[key] for sample in samples]
            if all(value == values[0] for value in values):
                continue
            self.stats["animated_attributes"] += 1
            if key == "transform":
                # Blades swing smoothly. Matrix entries are 24.8 fixed point,
                # so the tolerance sits a few steps above that quantisation;
                # the drift between keyframes stays under a screen pixel.
                indices = simplify(values, [0.012] * 4 + [0.6, 0.6])
                body = (
                    "".join(
                        f"{number(index * 100 / self.count)}%{{transform:{matrix(values[index])}}}"
                        for index in indices
                    )
                    + f"100%{{transform:{matrix(values[0])}}}"
                )
                # A blade and its edge highlight share one pose; share the CSS.
                name = self.motions.get(body)
                if name is None:
                    name = f"motion{len(self.motions)}"
                    self.motions[body] = name
                    self.styles.append(
                        f".{name}{{animation:{name} {number(self.duration)}s linear infinite;transform-origin:0 0}}@keyframes {name}{{{body}}}"
                    )
                element.set("class", name)
            elif isinstance(values[0], (float, int)):
                self.numbers(element, key, values, 0.004 if key == "opacity" else 0.45)
            else:
                letters = re.findall("[A-Za-z]", values[0])
                smooth = key == "d" and all(
                    re.findall("[A-Za-z]", value) == letters for value in values
                )
                self.steps(element, key, values, "linear" if smooth else "discrete")
        return element

    def node(self, samples):
        first = samples[0]
        assert all(sample["tag"] == first["tag"] for sample in samples)
        wrapper = ET.Element(
            "g",
            {
                "clip-path": f"url(#{self.rect_clip([sample['clip'] for sample in samples])})"
            },
        )
        refs = [sample.get("ref") for sample in samples]
        distinct = list(dict.fromkeys(refs))
        for ref in distinct:
            element = self.element(samples, ref)
            if len(distinct) > 1:
                # Changing text swaps between pre-built runs on the frames it
                # shows. The swap drives a wrapper's opacity, which every
                # browser animates, and leaves the run's own opacity alone.
                shown = [1 if value == ref else 0 for value in refs]
                state = ET.SubElement(wrapper, "g", {"opacity": str(shown[0])})
                self.steps(state, "opacity", shown)
                state.append(element)
                self.stats["text_states"] += 1
            else:
                wrapper.append(element)
        return wrapper

    def screen(self, parent, name, viewport, mask=None):
        screen = next(
            screen
            for screen in self.capture["screens"]
            if screen["theme"] == self.theme and screen["name"] == name
        )
        holder = parent
        if mask is not None:
            mask_name = self.name("screen")
            clip = ET.SubElement(
                self.defs,
                "clipPath",
                {"id": mask_name, "clipPathUnits": "userSpaceOnUse"},
            )
            clip.append(mask)
            holder = ET.SubElement(parent, "g", {"clip-path": f"url(#{mask_name})"})
        x, y, width, height = viewport
        canvas = ET.SubElement(
            holder,
            "svg",
            {
                "x": number(x),
                "y": number(y),
                "width": number(width),
                "height": number(height),
                "viewBox": f"0 0 {screen['width']} {screen['height']}",
                "preserveAspectRatio": "none",
            },
        )
        frames = align(screen["frames"])
        movie = ET.SubElement(canvas, "g", {"class": "movie"})
        poster = ET.SubElement(canvas, "g", {"class": "poster", "display": "none"})
        stacks = [[movie], [poster]]
        for index in range(len(frames[0])):
            raw = [frame[index] for frame in frames]
            shown = [sample is not None for sample in raw]
            samples = held(raw)
            tag = samples[0]["tag"]
            if tag == "clip-push":
                assert all(shown) and all(sample == samples[0] for sample in samples)
                path_name = self.name("path")
                clip = ET.SubElement(
                    self.defs,
                    "clipPath",
                    {"id": path_name, "clipPathUnits": "userSpaceOnUse"},
                )
                ET.SubElement(
                    clip,
                    "path",
                    {
                        "d": samples[0]["d"],
                        "transform": matrix(samples[0]["transform"]),
                        "clip-rule": samples[0]["fill-rule"],
                    },
                )
                for stack in stacks:
                    stack.append(
                        ET.SubElement(stack[-1], "g", {"clip-path": f"url(#{path_name})"})
                    )
            elif tag == "clip-pop":
                assert all(shown)
                for stack in stacks:
                    stack.pop()
            else:
                node = self.node(samples)
                if not all(shown):
                    # A draw that culling dropped on some frames, such as a
                    # notice parked outside its stage, shows only where drawn.
                    presence = ET.Element("g", {"opacity": str(int(shown[0]))})
                    self.steps(presence, "opacity", [int(value) for value in shown])
                    presence.append(node)
                    node = presence
                    self.stats["presence_states"] += 1
                stacks[0][-1].append(node)
                if shown[self.poster]:
                    stacks[1][-1].append(self.node([samples[self.poster]]))
        self.stats[f"{name}_draws"] = len(frames[0])
        return screen

    def build(self, logo):
        dark = self.theme == "dark"
        p = PALETTES[self.theme]
        svg = ET.Element(
            f"{{{SVG}}}svg",
            {
                "viewBox": f"0 0 {WIDTH} {HEIGHT}",
                "width": str(WIDTH),
                "height": str(HEIGHT),
                "role": "img",
                "aria-labelledby": "title description",
            },
        )
        ET.SubElement(svg, "title", {"id": "title"}).text = "mirui"
        ET.SubElement(svg, "desc", {"id": "description"}).text = (
            "The Iris lens demo rendered by mirui at four resolutions: a 128x160 "
            "ESP32-C3 panel, a 240x240 round watch, a 390x844 phone and a 1280x800 "
            "browser window. All four close their aperture blades and capture together."
        )
        style = ET.SubElement(svg, "style")
        svg.append(self.defs)
        self.resources()
        d = Draw(self.defs, p["shadow"])

        # Stage: a plain neutral surface with one soft, colourless key light.
        ET.SubElement(
            svg, "rect", {"width": str(WIDTH), "height": str(HEIGHT), "rx": "24", "fill": p["bg"]}
        )
        for cx, cy, r, color, alpha in [(760, 420, 620, p["key"], p["key_alpha"])]:
            ET.SubElement(
                svg,
                "circle",
                {"cx": str(cx), "cy": str(cy), "r": str(r), "fill": d.glow(color, alpha)},
            )

        brand = ET.SubElement(
            svg, "g", {"transform": f"translate(52 50) scale({number(76 / 1230)})"}
        )
        for child in ET.parse(logo).getroot():
            if child.tag.endswith("}g"):
                brand.append(child)
        chip = ET.SubElement(svg, "g", {"transform": "translate(52 150)"})
        ET.SubElement(
            chip,
            "rect",
            {"width": "176", "height": "32", "rx": "16", "fill": p["chip"], "stroke": p["edge"]},
        )
        text = ET.SubElement(
            chip,
            "text",
            {"x": "18", "y": "21", "font-family": MONO, "font-size": "13", "fill": p["ink"]},
        )
        ET.SubElement(text, "tspan", {"fill": "#e5533b"}).text = "$ "
        ET.SubElement(text, "tspan").text = "cargo add mirui"

        # A ruler under the line-up: every device sits on the same baseline.
        ruler = ET.SubElement(svg, "g", {"stroke": p["faint"], "stroke-width": "1"})
        ET.SubElement(ruler, "line", {"x1": "52", "y1": "583.5", "x2": "1228", "y2": "583.5"})
        for x in range(52, 1229, 24):
            ET.SubElement(
                ruler, "line", {"x1": str(x + 0.5), "y1": "583.5", "x2": str(x + 0.5), "y2": "587.5"}
            )

        devices = [
            self.board(svg, d, p, 52, 332),
            self.watch(svg, d, p, 333, 452),
            self.phone(svg, d, p, 458, 199),
            self.browser(svg, d, p, 664, 186),
        ]
        for center, size, target in devices:
            ET.SubElement(
                ruler, "line", {"x1": str(center), "y1": "578", "x2": str(center), "y2": "590", "stroke": p["quiet"]}
            )
            label = ET.SubElement(
                svg,
                "text",
                {
                    "x": str(center),
                    "y": "608",
                    "text-anchor": "middle",
                    "font-family": MONO,
                    "font-size": "13",
                    "fill": p["ink"],
                },
            )
            label.text = size
            ET.SubElement(
                svg,
                "text",
                {
                    "x": str(center),
                    "y": "625",
                    "text-anchor": "middle",
                    "font-family": MONO,
                    "font-size": "10.5",
                    "letter-spacing": "0.4",
                    "fill": p["quiet"],
                },
            ).text = target

        style.text = (
            ".poster{display:none}@media(prefers-reduced-motion:reduce){.movie,.motion{display:none}.poster{display:inline}}"
            + "".join(self.styles)
        )
        return ET.tostring(svg, encoding="unicode")

    def flash(self, parent, shape):
        """White capture flash over a screen, timed to the shutter."""
        loop = self.capture["loop_ms"] / 1000
        at = self.capture["capture_ms"] / 1000 + 0.06
        shape.set("fill", "#ffffff")
        shape.set("opacity", "0")
        shape.set("class", "motion")
        ET.SubElement(
            shape,
            "animate",
            {
                "attributeName": "opacity",
                "dur": f"{number(loop)}s",
                "repeatCount": "indefinite",
                "calcMode": "linear",
                "keyTimes": f"0;{number(at / loop)};{number((at + 0.05) / loop)};{number((at + 0.42) / loop)};1",
                "values": "0;0;0.22;0;0",
            },
        )
        parent.append(shape)

    def board(self, svg, d, p, x, y):
        """ESP32-C3 dev board with a 1.8 inch ST7735 panel, 128 x 160 at 1:1."""
        g = ET.SubElement(svg, "g")
        d.shadow(g, x + 78, 586, 92, 9)
        ET.SubElement(g, "rect", {"x": str(x), "y": str(y), "width": "156", "height": "236", "rx": "9", "fill": "#10161d", "stroke": "#26313d"})
        for hx, hy in [(x + 9, y + 9), (x + 147, y + 9), (x + 9, y + 227), (x + 147, y + 227)]:
            ET.SubElement(g, "circle", {"cx": str(hx), "cy": str(hy), "r": "3.6", "fill": "#07090c", "stroke": "#b8913f", "stroke-width": "1.4"})
        for row in range(9):
            for px in (x + 5, x + 151):
                ET.SubElement(g, "circle", {"cx": str(px), "cy": str(y + 34 + row * 16), "r": "2", "fill": "#c79d45"})
        ET.SubElement(g, "rect", {"x": str(x + 12), "y": str(y + 12), "width": "132", "height": "172", "rx": "3", "fill": "#040506", "stroke": "#1b2129"})
        screen = self.screen(g, "panel", (x + 14, y + 18, 128, 160))
        self.flash(g, ET.Element("rect", {"x": str(x + 14), "y": str(y + 18), "width": "128", "height": "160"}))
        d.glare(g, ET.Element("rect", {"x": str(x + 12), "y": str(y + 12), "width": "132", "height": "172", "rx": "3"}))
        ET.SubElement(g, "rect", {"x": str(x + 14), "y": str(y + 194), "width": "70", "height": "30", "rx": "2.5", "fill": d.metal()})
        ET.SubElement(g, "text", {"x": str(x + 20), "y": str(y + 213), "font-family": MONO, "font-size": "8.5", "fill": "#3d434c"}).text = "ESP32-C3"
        for bx in (x + 98, x + 124):
            ET.SubElement(g, "rect", {"x": str(bx), "y": str(y + 199), "width": "18", "height": "18", "rx": "3", "fill": "#1d242d"})
            ET.SubElement(g, "circle", {"cx": str(bx + 9), "cy": str(y + 208), "r": "5", "fill": "#0b0e12", "stroke": "#39434f"})
        ET.SubElement(g, "rect", {"x": str(x + 60), "y": str(y + 229), "width": "36", "height": "11", "rx": "4", "fill": d.metal()})
        return x + 78, f"{screen['width']} × {screen['height']}", "ESP32-C3 · ST7735"

    def watch(self, svg, d, p, cx, cy):
        """Round watch face, 240 x 240 shown at 0.74."""
        g = ET.SubElement(svg, "g")
        scale = 0.74
        r = 120 * scale
        # Short tapered strap ends that fade out, so the case reads as a watch.
        for direction in (-1, 1):
            band = d.fade(p["strap"], direction)
            near = cy + direction * (r + 4)
            far = cy + direction * (r + 62)
            ET.SubElement(
                g,
                "path",
                {
                    "d": f"M{number(cx - 54)} {number(near)}L{number(cx - 44)} {number(far)}H{number(cx + 44)}L{number(cx + 54)} {number(near)}Z",
                    "fill": band,
                },
            )
        d.shadow(g, cx, 586, 96, 8)
        ET.SubElement(g, "rect", {"x": str(cx + r + 8), "y": str(cy - 20), "width": "13", "height": "40", "rx": "5", "fill": d.metal()})
        ET.SubElement(g, "rect", {"x": str(cx + r + 6), "y": str(cy + 32), "width": "8", "height": "22", "rx": "3", "fill": d.metal()})
        ET.SubElement(g, "circle", {"cx": str(cx), "cy": str(cy), "r": str(r + 16), "fill": d.case(dark=p["dark"])})
        ET.SubElement(g, "circle", {"cx": str(cx), "cy": str(cy), "r": str(r + 15.5), "fill": "none", "stroke": p["case_hi"], "stroke-opacity": "0.55"})
        ET.SubElement(g, "circle", {"cx": str(cx), "cy": str(cy), "r": str(r + 3.5), "fill": "#020203"})
        mask = ET.Element("circle", {"cx": number(cx), "cy": number(cy), "r": number(r)})
        screen = self.screen(g, "dial", (cx - r, cy - r, 2 * r, 2 * r), mask)
        self.flash(g, ET.Element("circle", {"cx": number(cx), "cy": number(cy), "r": number(r)}))
        d.glare(g, ET.Element("circle", {"cx": number(cx), "cy": number(cy), "r": number(r + 3.5)}))
        return cx, f"{screen['width']} × {screen['height']}", "NuttX · round 1.28\""

    def phone(self, svg, d, p, x, y):
        """Phone, 390 x 844 shown at 0.42."""
        g = ET.SubElement(svg, "g")
        scale = 0.42
        w, h = 390 * scale, 844 * scale
        d.shadow(g, x + 89, 586, 104, 9)
        ET.SubElement(g, "rect", {"x": str(x - 2), "y": str(y + 88), "width": "3", "height": "28", "rx": "1.5", "fill": p["frame_edge"]})
        ET.SubElement(g, "rect", {"x": str(x + 176.5), "y": str(y + 104), "width": "3", "height": "44", "rx": "1.5", "fill": p["frame_edge"]})
        ET.SubElement(g, "rect", {"x": str(x), "y": str(y), "width": "178", "height": "369", "rx": "29", "fill": p["frame"], "stroke": p["frame_edge"]})
        ET.SubElement(g, "rect", {"x": str(x + 6), "y": str(y + 6), "width": "166", "height": "357", "rx": "24", "fill": "#020203"})
        sx, sy = x + 7, y + 7.25
        mask = ET.Element("rect", {"x": number(sx), "y": number(sy), "width": number(w), "height": number(h), "rx": "22"})
        screen = self.screen(g, "phone", (sx, sy, w, h), mask)
        self.flash(g, ET.Element("rect", {"x": number(sx), "y": number(sy), "width": number(w), "height": number(h), "rx": "22"}))
        ET.SubElement(g, "rect", {"x": str(x + 69), "y": str(y + 14), "width": "40", "height": "11", "rx": "5.5", "fill": "#000000"})
        d.glare(g, ET.Element("rect", {"x": str(x + 6), "y": str(y + 6), "width": "166", "height": "357", "rx": "24"}))
        self.tap(g, screen, (sx, sy, scale), 0.0)
        return x + 89, f"{screen['width']} × {screen['height']}", "Android · WGPU"

    def browser(self, svg, d, p, x, y):
        """Browser window on the web build, 1280 x 800 shown at 0.44."""
        g = ET.SubElement(svg, "g")
        scale = 0.44
        w, h = 1280 * scale, 800 * scale
        d.shadow(g, x + w / 2, 588, w * 0.52, 12)
        ET.SubElement(g, "rect", {"x": str(x), "y": str(y), "width": number(w), "height": number(h + 30), "rx": "12", "fill": p["window"], "stroke": p["frame_edge"]})
        for index in range(3):
            ET.SubElement(g, "circle", {"cx": str(x + 17 + index * 14), "cy": str(y + 15), "r": "4.2", "fill": p["dots"]})
        ET.SubElement(g, "rect", {"x": number(x + w / 2 - 120), "y": str(y + 7), "width": "240", "height": "16", "rx": "8", "fill": p["address"]})
        ET.SubElement(g, "text", {"x": number(x + w / 2), "y": str(y + 18.5), "text-anchor": "middle", "font-family": MONO, "font-size": "9.5", "fill": p["quiet"]}).text = "mirui.rs/?demo=iris"
        sx, sy = x, y + 30
        mask = ET.Element("path", {"d": f"M{number(sx)} {number(sy)}h{number(w)}v{number(h - 12)}a12 12 0 0 1 -12 12h{number(-(w - 24))}a12 12 0 0 1 -12 -12Z"})
        screen = self.screen(g, "desktop", (sx, sy, w, h), mask)
        self.flash(g, ET.Element("rect", {"x": number(sx), "y": number(sy), "width": number(w), "height": number(h)}))
        self.cursor(g, screen, (sx, sy, scale))
        return x + w / 2, f"{screen['width']} × {screen['height']}", "Browser · Web Canvas"

    def tap(self, parent, screen, origin, delay):
        """A soft ring where a finger presses the shutter on touch screens."""
        if not screen.get("shutter"):
            return
        sx, sy, scale = origin
        bx, by, bw, bh = screen["shutter"]
        cx, cy = sx + (bx + bw / 2) * scale, sy + (by + bh / 2) * scale
        loop = self.capture["loop_ms"] / 1000
        at = self.capture["capture_ms"] / 1000 - 0.12 + delay
        ring = ET.SubElement(parent, "circle", {"cx": number(cx), "cy": number(cy), "r": number(bw * scale * 0.62), "fill": "#ffffff", "opacity": "0", "class": "motion"})
        ET.SubElement(ring, "animate", {"attributeName": "opacity", "dur": f"{number(loop)}s", "repeatCount": "indefinite", "keyTimes": f"0;{number(at / loop)};{number((at + 0.08) / loop)};{number((at + 0.5) / loop)};1", "values": "0;0;0.32;0;0"})
        ET.SubElement(ring, "animate", {"attributeName": "r", "dur": f"{number(loop)}s", "repeatCount": "indefinite", "keyTimes": f"0;{number(at / loop)};{number((at + 0.5) / loop)};1", "values": f"{number(bw * scale * 0.5)};{number(bw * scale * 0.5)};{number(bw * scale * 0.95)};{number(bw * scale * 0.95)}"})

    def cursor(self, parent, screen, origin):
        """Pointer that clicks the desktop shutter at the capture moment."""
        if not screen.get("shutter"):
            return
        sx, sy, scale = origin
        bx, by, bw, bh = screen["shutter"]
        tx, ty = sx + (bx + bw / 2) * scale, sy + (by + bh / 2) * scale
        loop = self.capture["loop_ms"] / 1000
        at = self.capture["capture_ms"] / 1000
        name = f"cursor{len(self.styles)}"
        # Enter from the lens stage, press the shutter, then drift back out.
        keys = [
            (0.0, tx - 230, ty - 150, 0, 1),
            (at - 1.15, tx - 230, ty - 150, 0, 1),
            (at - 0.95, tx - 200, ty - 132, 1, 1),
            (at - 0.18, tx + 3, ty + 3, 1, 1),
            (at - 0.12, tx + 3, ty + 3, 1, 0.86),
            (at + 0.02, tx + 3, ty + 3, 1, 1),
            (at + 0.8, tx + 3, ty + 3, 1, 1),
            (at + 1.6, tx - 160, ty - 120, 0, 1),
            (loop, tx - 230, ty - 150, 0, 1),
        ]
        frames = "".join(
            f"{number(t * 100 / loop)}%{{transform:translate({number(px)}px,{number(py)}px) scale({number(s)});opacity:{o}}}"
            for t, px, py, o, s in keys
        )
        self.styles.append(
            f".{name}{{animation:{name} {number(loop)}s cubic-bezier(.45,0,.2,1) infinite}}@keyframes {name}{{{frames}}}"
        )
        pointer = ET.SubElement(parent, "g", {"class": f"{name} motion", "opacity": "0"})
        ET.SubElement(
            pointer,
            "path",
            {
                "d": "M0 0L0 17L4.3 13L7.4 19.6L10.3 18.3L7.3 11.9L13 11.9Z",
                "fill": "#ffffff",
                "stroke": "#0b0d10",
                "stroke-width": "1.3",
                "stroke-linejoin": "round",
            },
        )


class Draw:
    """Shared paints for the device line-up."""

    def __init__(self, defs, depth):
        self.defs = defs
        self.cache = {}
        self.depth = depth

    def define(self, key, build):
        if key not in self.cache:
            name = f"hero{len(self.cache)}"
            build(name)
            self.cache[key] = name
        return f"url(#{self.cache[key]})"

    def glow(self, color, alpha):
        def build(name):
            g = ET.SubElement(self.defs, "radialGradient", {"id": name})
            for offset, opacity in [(0, alpha), (0.45, alpha * 0.4), (1, 0)]:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": color, "stop-opacity": number(opacity)})

        return self.define(("glow", color, alpha), build)

    def metal(self):
        def build(name):
            g = ET.SubElement(self.defs, "linearGradient", {"id": name, "x1": "0", "y1": "0", "x2": "0", "y2": "1"})
            for offset, color in [(0, "#e4e8ed"), (0.5, "#a9b0ba"), (1, "#7a828d")]:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": color})

        return self.define(("metal",), build)

    def case(self, dark):
        def build(name):
            g = ET.SubElement(self.defs, "linearGradient", {"id": name, "x1": "1", "y1": "0", "x2": "0", "y2": "1"})
            stops = (
                [(0, "#4a515c"), (0.45, "#1c2027"), (1, "#0a0b0e")]
                if dark
                else [(0, "#ffffff"), (0.5, "#d5dae1"), (1, "#a7b0bb")]
            )
            for offset, color in stops:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": color})

        return self.define(("case", dark), build)

    def fade(self, color, direction):
        def build(name):
            g = ET.SubElement(self.defs, "linearGradient", {"id": name, "x1": "0", "y1": "0", "x2": "0", "y2": "1"})
            stops = [(0, 0), (1, 0.9)] if direction < 0 else [(0, 0.9), (1, 0)]
            for offset, opacity in stops:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": color, "stop-opacity": number(opacity)})

        return self.define(("fade", color, direction), build)

    def shadow(self, parent, cx, cy, rx, ry):
        def build(name):
            g = ET.SubElement(self.defs, "radialGradient", {"id": name})
            for offset, opacity in [(0, self.depth), (1, 0)]:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": "#000000", "stop-opacity": number(opacity)})

        paint = self.define(("shadow",), build)
        ET.SubElement(parent, "ellipse", {"cx": number(cx), "cy": number(cy), "rx": number(rx), "ry": number(ry), "fill": paint})

    def glare(self, parent, shape):
        """A faint diagonal reflection on cover glass."""

        def build(name):
            g = ET.SubElement(self.defs, "linearGradient", {"id": name, "x1": "0", "y1": "0", "x2": "1", "y2": "1"})
            for offset, opacity in [(0, 0.10), (0.38, 0.035), (0.39, 0), (1, 0)]:
                ET.SubElement(g, "stop", {"offset": number(offset), "stop-color": "#ffffff", "stop-opacity": number(opacity)})

        shape.set("fill", self.define(("glare",), build))
        shape.set("pointer-events", "none")
        parent.append(shape)


def export_heroes(source, output):
    capture = json.loads(source.read_text())
    report = {}
    for theme in ["light", "dark"]:
        hero = Hero(capture, theme)
        target = output / f"readme-hero-{theme}.svg"
        target.write_text(hero.build(output / f"logo-{theme}.svg") + "\n")
        report[theme] = dict(
            bytes=target.stat().st_size, seconds=hero.duration, **hero.stats
        )
    print(json.dumps(report))


def export_logos(source, output, evidence):
    import numpy as np
    import vtracer
    from PIL import Image
    from psd_tools import PSDImage

    document = PSDImage.open(source)
    groups = {layer.name.split()[0]: layer for layer in document if layer.is_group()}
    report = {"source_sha256": sha256(source.read_bytes()).hexdigest(), "themes": {}}
    output.mkdir(parents=True, exist_ok=True)
    for theme in ["light", "dark"]:
        layers = [
            layer
            for layer in groups[theme].descendants()
            if not layer.is_group() and layer.name.split()[0] != "bg"
        ]
        left = min(layer.left for layer in layers)
        top = min(layer.top for layer in layers)
        right = max(layer.right for layer in layers)
        bottom = max(layer.bottom for layer in layers)
        svg = ET.Element(
            f"{{{SVG}}}svg",
            {
                "viewBox": f"0 0 {right - left} {bottom - top}",
                "role": "img",
                "aria-labelledby": "title",
            },
        )
        ET.SubElement(svg, f"{{{SVG}}}title", {"id": "title"}).text = "mirui"
        details = []
        with tempfile.TemporaryDirectory(prefix="mirui-logo-") as directory:
            directory = Path(directory)
            for index, layer in enumerate(layers):
                pixels = np.asarray(layer.topil().convert("RGBA"))
                opaque = pixels[:, :, 3] > 250
                colors = Counter(map(tuple, pixels[:, :, :3][opaque].tolist()))
                color, count = colors.most_common(1)[0]
                assert count / int(opaque.sum()) > 0.99, layer.name
                mask = np.where(pixels[:, :, 3] >= 128, 0, 255).astype(np.uint8)
                image_path = directory / f"{index}.png"
                vector_path = directory / f"{index}.svg"
                Image.fromarray(mask).save(image_path)
                vtracer.convert_image_to_svg_py(
                    str(image_path),
                    str(vector_path),
                    colormode="binary",
                    mode="spline",
                    filter_speckle=4,
                    corner_threshold=60,
                    length_threshold=1.0,
                    max_iterations=10,
                    splice_threshold=45,
                    path_precision=3,
                )
                ink = "#" + "".join(f"{channel:02x}" for channel in color)
                group = ET.SubElement(
                    svg,
                    f"{{{SVG}}}g",
                    {
                        "transform": f"translate({layer.left - left} {layer.top - top})",
                        "fill": ink,
                    },
                )
                paths = ET.parse(vector_path).getroot().findall(f"{{{SVG}}}path")
                assert paths, layer.name
                for path in paths:
                    path.attrib.pop("fill", None)
                    group.append(path)
                details.append(
                    {
                        "layer": layer.name.split()[0],
                        "fill": ink,
                        "paths": len(paths),
                        "alpha_sha256": sha256(pixels[:, :, 3].tobytes()).hexdigest(),
                    }
                )
        target = output / f"logo-{theme}.svg"
        ET.indent(svg, space="  ")
        ET.ElementTree(svg).write(target, encoding="unicode")
        report["themes"][theme] = {
            "bounds": [left, top, right, bottom],
            "layers": details,
            "bytes": target.stat().st_size,
        }
    evidence.parent.mkdir(parents=True, exist_ok=True)
    evidence.write_text(json.dumps(report, indent=2) + "\n")
    print(
        json.dumps(
            {theme: report["themes"][theme]["bytes"] for theme in report["themes"]}
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--logo-psd", type=Path)
    parser.add_argument("--capture", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "docs/assets")
    parser.add_argument(
        "--evidence", type=Path, default=ROOT / ".local/readme-hero/logo-export.json"
    )
    args = parser.parse_args()
    if args.logo_psd:
        export_logos(args.logo_psd, args.output, args.evidence)
    if args.capture:
        export_heroes(args.capture, args.output)


if __name__ == "__main__":
    main()
