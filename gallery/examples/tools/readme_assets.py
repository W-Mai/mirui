#!/usr/bin/env python3
"""Export the layered wordmark and assemble the README's SVG artwork."""

import argparse
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


def matrix(values):
    return "matrix(" + ",".join(number(value) for value in values) + ")"


def attribute(value):
    if isinstance(value, (float, int)):
        return number(value)
    return str(value)


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


class Hero:
    def __init__(self, capture, theme):
        self.capture = capture
        self.theme = theme
        self.styles = []
        self.clips = {}
        self.defs = ET.Element("defs")
        self.duration = len(capture["screens"][0]["frames"]) / capture["fps"]
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
            self.clips[tuple(rect)] = name
        return self.clips[key]

    def node(self, samples, animated):
        first = samples[0]
        assert all(
            sample.keys() == first.keys() and sample["tag"] == first["tag"]
            for sample in samples
        )
        assert all(sample["clip"] == first["clip"] for sample in samples)
        attrs = {
            key: value
            for key, value in first.items()
            if key not in ["tag", "clip", "ref"]
        }
        if "ref" in first:
            assert all(sample["ref"] == first["ref"] for sample in samples)
            attrs["href"] = f"#d{first['ref']}"
        attrs["transform"] = matrix(first["transform"])
        element = ET.Element(
            first["tag"], {key: attribute(value) for key, value in attrs.items()}
        )
        if animated:
            count = len(samples)
            for key in sorted(first.keys() - {"tag", "clip", "ref"}):
                values = [sample[key] for sample in samples]
                if all(value == values[0] for value in values):
                    continue
                self.stats["animated_attributes"] += 1
                if key == "transform":
                    indices = simplify(values, [0.00008] * 4 + [0.08, 0.08])
                    name = f"motion{len(self.styles)}"
                    body = (
                        "".join(
                            f"{number(index * 100 / count)}%{{transform:{matrix(values[index])}}}"
                            for index in indices
                        )
                        + f"100%{{transform:{matrix(values[0])}}}"
                    )
                    self.styles.append(
                        f".{name}{{animation:{name} {number(self.duration)}s linear infinite;transform-origin:0 0}}@keyframes {name}{{{body}}}"
                    )
                    element.set("class", name)
                    continue
                if isinstance(values[0], (float, int)):
                    indices = simplify(
                        [[value] for value in values],
                        [0.001 if key == "opacity" else 0.08],
                    )
                    mode = "linear"
                else:
                    indices = [0] + [
                        index
                        for index in range(1, count)
                        if values[index] != values[index - 1]
                    ]
                    mode = (
                        "linear"
                        if key == "d"
                        and all(
                            re.findall("[A-Za-z]", value)
                            == re.findall("[A-Za-z]", values[0])
                            for value in values
                        )
                        else "discrete"
                    )
                ET.SubElement(
                    element,
                    "animate",
                    {
                        "attributeName": key,
                        "dur": f"{number(self.duration)}s",
                        "repeatCount": "indefinite",
                        "calcMode": mode,
                        "keyTimes": ";".join(number(index / count) for index in indices)
                        + ";1",
                        "values": ";".join(
                            attribute(values[index]) for index in indices
                        )
                        + ";"
                        + attribute(values[0]),
                    },
                )
        wrapper = ET.Element("g", {"clip-path": f"url(#{self.clip(first['clip'])})"})
        wrapper.append(element)
        return wrapper

    def screen(self, parent, name, viewport):
        screen = next(
            screen
            for screen in self.capture["screens"]
            if screen["theme"] == self.theme and screen["orientation"] == name
        )
        canvas = ET.SubElement(
            parent,
            "svg",
            {
                key: attribute(value)
                for key, value in zip(["x", "y", "width", "height"], viewport)
            },
        )
        canvas.set("viewBox", f"0 0 {screen['width']} {screen['height']}")
        frames = screen["frames"]
        assert all(len(frame) == len(frames[0]) for frame in frames)
        movie = ET.SubElement(canvas, "g", {"class": "movie"})
        poster = ET.SubElement(canvas, "g", {"class": "poster", "display": "none"})
        for index in range(len(frames[0])):
            movie.append(self.node([frame[index] for frame in frames], True))
            poster.append(self.node([frames[0][index]], False))
        self.stats[f"{name}_draws"] = len(frames[0])

    def build(self, logo):
        dark = self.theme == "dark"
        background = "#0d1117" if dark else "#f6f8fc"
        frame = "#1b2330" if dark else "#ffffff"
        edge = "#344052" if dark else "#dce2eb"
        ink = "#e6edf3" if dark else "#39424c"
        quiet = "#8f9fb3" if dark else "#738197"
        svg = ET.Element(
            f"{{{SVG}}}svg",
            {
                "viewBox": "0 0 1200 850",
                "width": "1200",
                "height": "850",
                "role": "img",
                "aria-labelledby": "title description",
            },
        )
        ET.SubElement(
            svg, "title", {"id": "title"}
        ).text = "mirui — one interface, wide and compact"
        ET.SubElement(
            svg, "desc", {"id": "description"}
        ).text = "Orbit Console running side by side in landscape and portrait, with live vector animation."
        style = ET.SubElement(svg, "style")
        svg.append(self.defs)
        self.resources()
        shadow = ET.SubElement(
            self.defs,
            "filter",
            {
                "id": "shadow",
                "x": "-30%",
                "y": "-30%",
                "width": "160%",
                "height": "180%",
                "color-interpolation-filters": "sRGB",
            },
        )
        ET.SubElement(shadow, "feGaussianBlur", {"stdDeviation": "16"})
        ET.SubElement(
            svg,
            "rect",
            {"width": "1200", "height": "850", "rx": "28", "fill": background},
        )
        brand = ET.SubElement(svg, "g", {"transform": "translate(62 38) scale(0.093)"})
        for child in ET.parse(logo).getroot():
            if child.tag.endswith("}g"):
                brand.append(child)
        text = ET.SubElement(
            svg,
            "text",
            {
                "x": "759",
                "y": "92",
                "fill": ink,
                "font-family": "Arial,Helvetica,sans-serif",
                "font-size": "30",
                "font-weight": "500",
                "letter-spacing": "-0.7",
            },
        )
        text.text = "An interface for"
        ET.SubElement(text, "tspan", {"x": "759", "dy": "39"}).text = "every screen."
        ET.SubElement(
            svg,
            "text",
            {
                "x": "64",
                "y": "194",
                "fill": quiet,
                "font-family": "Arial,Helvetica,sans-serif",
                "font-size": "13",
                "letter-spacing": "2.1",
            },
        ).text = "EMBEDDED  /  MOBILE  /  DESKTOP  /  WEB"
        for orientation, x, y, width, height, radius in [
            ("wide", 48, 218, 860, 568, 20),
            ("tall", 886, 282, 254, 478, 32),
        ]:
            device = ET.SubElement(svg, "g")
            ET.SubElement(
                device,
                "rect",
                {
                    "x": str(x),
                    "y": str(y + 16),
                    "width": str(width),
                    "height": str(height),
                    "rx": str(radius),
                    "fill": "#000000",
                    "opacity": "0.46" if dark else "0.13",
                    "filter": "url(#shadow)",
                },
            )
            ET.SubElement(
                device,
                "rect",
                {
                    "x": str(x),
                    "y": str(y),
                    "width": str(width),
                    "height": str(height),
                    "rx": str(radius),
                    "fill": frame,
                    "stroke": edge,
                },
            )
            if orientation == "wide":
                for cx in [70, 86, 102]:
                    ET.SubElement(
                        device,
                        "circle",
                        {"cx": str(cx), "cy": "238", "r": "3.7", "fill": edge},
                    )
                ET.SubElement(
                    device,
                    "text",
                    {
                        "x": "462",
                        "y": "242",
                        "text-anchor": "middle",
                        "fill": quiet,
                        "font-family": "Arial,Helvetica,sans-serif",
                        "font-size": "10",
                        "letter-spacing": "1.5",
                    },
                ).text = "ORBIT CONSOLE"
                self.screen(device, "wide", (60, 258, 836, 522.5))
            else:
                ET.SubElement(
                    device,
                    "rect",
                    {
                        "x": "974",
                        "y": "293",
                        "width": "78",
                        "height": "5",
                        "rx": "2.5",
                        "fill": edge,
                    },
                )
                ET.SubElement(
                    device,
                    "rect",
                    {
                        "x": "975",
                        "y": "742",
                        "width": "76",
                        "height": "4",
                        "rx": "2",
                        "fill": edge,
                    },
                )
                self.screen(device, "tall", (899, 315, 228, 405.33333))
        style.text = (
            ".poster{display:none}@media(prefers-reduced-motion:reduce){.movie{display:none}.poster{display:inline}}"
            + "".join(self.styles)
        )
        return ET.tostring(svg, encoding="unicode")


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
