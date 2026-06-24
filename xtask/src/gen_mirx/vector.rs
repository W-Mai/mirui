//! `gen-mirx vector` — pack a text scene description into a VECTOR chunk.
//!
//! Line-based grammar, one op per line, `#` comments and blank lines
//! ignored:
//!
//!   rect   <x> <y> <w> <h> <radius> <r> <g> <b> <a> <opa>
//!   border <x> <y> <w> <h> <width> <radius> <r> <g> <b> <a> <opa>
//!   line   <x1> <y1> <x2> <y2> <width> <r> <g> <b> <a> <opa>
//!   arc    <cx> <cy> <radius> <start_deg> <end_deg> <width> <r> <g> <b> <a> <opa>
//!   label  <token> <x> <y> <r> <g> <b> <a> <opa> <text>
//!   blit   <token> <px> <py> <sx> <sy>
//!   group  <tx> <ty> [<opacity>] [disjoint]
//!   endgroup
//!
//! Coordinates are decimal (parsed into 24.8 fixed-point); colours and
//! opacity are 0-255. `<token>` and `<text>` are single tokens — no
//! whitespace inside them. `fill_path` stays macro-only because the path
//! sub-grammar is awkward in a flat line-oriented file.

use std::fs;
use std::path::PathBuf;

use mirui::render::scene::codec::encode_scene;
use mirui::render::scene::{ResourceRef, SceneOp};
use mirui::types::{Color, Fixed, Point, Rect, Transform};
use mirx::{ChunkEntry, chunk_type, encode_chunk_generic};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(args: &[String]) -> Result {
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--in" => {
                input = Some(PathBuf::from(args.get(i + 1).ok_or("--in needs a value")?));
                i += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(args.get(i + 1).ok_or("--out needs a value")?));
                i += 2;
            }
            other => return Err(format!("unexpected arg: {other}").into()),
        }
    }
    let input = input.ok_or("missing --in")?;
    let out = out.ok_or("missing --out")?;

    let text = fs::read_to_string(&input)?;
    let ops = parse_scene(&text)?;
    let payload = encode_scene(&ops).map_err(|e| format!("encode failed: {e:?}"))?;
    let bytes = encode_chunk_generic(chunk_type::VECTOR, ChunkEntry::FLAG_CRITICAL, &payload);
    fs::write(&out, &bytes)?;

    println!(
        "wrote {} bytes to {} ({} ops)",
        bytes.len(),
        out.display(),
        ops.len(),
    );
    Ok(())
}

fn parse_scene(text: &str) -> Result<Vec<SceneOp>> {
    let mut ops = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut f = line.split_whitespace();
        let kind = f.next().unwrap();
        let rest: Vec<&str> = f.collect();
        let op = parse_op(kind, &rest).map_err(|e| format!("line {}: {e}", lineno + 1))?;
        ops.push(op);
    }
    Ok(ops)
}

fn parse_op(kind: &str, a: &[&str]) -> Result<SceneOp> {
    match kind {
        "rect" => {
            expect(a, 10, "rect")?;
            Ok(SceneOp::FillRect {
                area: rect(a, 0)?,
                transform: Transform::IDENTITY,
                quad: None,
                color: color(a, 5)?,
                radius: fixed(a[4])?,
                opa: byte(a[9])?,
            })
        }
        "line" => {
            expect(a, 10, "line")?;
            Ok(SceneOp::Line {
                p1: point(a, 0)?,
                p2: point(a, 2)?,
                transform: Transform::IDENTITY,
                color: color(a, 5)?,
                width: fixed(a[4])?,
                opa: byte(a[9])?,
            })
        }
        "arc" => {
            expect(a, 11, "arc")?;
            Ok(SceneOp::Arc {
                center: point(a, 0)?,
                transform: Transform::IDENTITY,
                radius: fixed(a[2])?,
                start_angle: fixed(a[3])?,
                end_angle: fixed(a[4])?,
                width: fixed(a[5])?,
                color: color(a, 6)?,
                opa: byte(a[10])?,
            })
        }
        "border" => {
            expect(a, 11, "border")?;
            Ok(SceneOp::Border {
                area: rect(a, 0)?,
                transform: Transform::IDENTITY,
                quad: None,
                width: fixed(a[4])?,
                radius: fixed(a[5])?,
                color: color(a, 6)?,
                opa: byte(a[10])?,
            })
        }
        "label" => {
            expect(a, 9, "label")?;
            Ok(SceneOp::Label {
                font: ResourceRef::Token(a[0].to_owned().into()),
                pos: point(a, 1)?,
                transform: Transform::IDENTITY,
                color: color(a, 3)?,
                opa: byte(a[7])?,
                text: a[8].to_owned().into(),
            })
        }
        "blit" => {
            expect(a, 5, "blit")?;
            Ok(SceneOp::Blit {
                texture: ResourceRef::Token(a[0].to_owned().into()),
                pos: point(a, 1)?,
                size: point(a, 3)?,
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
            })
        }
        "group" => {
            if a.len() < 2 || a.len() > 4 {
                return Err(format!("`group` expects 2..=4 args, got {}", a.len()).into());
            }
            let opacity = if a.len() >= 3 {
                Some(byte(a[2])?)
            } else {
                None
            };
            let disjoint_hint = a.len() == 4 && a[3] == "disjoint";
            if a.len() == 4 && !disjoint_hint {
                return Err(
                    format!("`group` 4th arg must be literal `disjoint`, got `{}`", a[3]).into(),
                );
            }
            Ok(SceneOp::GroupBegin {
                transform: Some(Transform::translate(fixed(a[0])?, fixed(a[1])?)),
                opacity,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint,
            })
        }
        "endgroup" => {
            expect(a, 0, "endgroup")?;
            Ok(SceneOp::GroupEnd)
        }
        other => Err(format!("unknown op `{other}`").into()),
    }
}

fn expect(a: &[&str], n: usize, kind: &str) -> Result {
    if a.len() != n {
        return Err(format!("`{kind}` expects {n} args, got {}", a.len()).into());
    }
    Ok(())
}

fn fixed(s: &str) -> Result<Fixed> {
    Ok(Fixed::from_f32(s.parse::<f32>()?))
}

fn byte(s: &str) -> Result<u8> {
    Ok(s.parse::<u8>()?)
}

fn point(a: &[&str], i: usize) -> Result<Point> {
    Ok(Point {
        x: fixed(a[i])?,
        y: fixed(a[i + 1])?,
    })
}

fn rect(a: &[&str], i: usize) -> Result<Rect> {
    Ok(Rect {
        x: fixed(a[i])?,
        y: fixed(a[i + 1])?,
        w: fixed(a[i + 2])?,
        h: fixed(a[i + 3])?,
    })
}

fn color(a: &[&str], i: usize) -> Result<Color> {
    Ok(Color {
        r: byte(a[i])?,
        g: byte(a[i + 1])?,
        b: byte(a[i + 2])?,
        a: byte(a[i + 3])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::render::scene::codec::decode_scene;

    #[test]
    fn parse_encode_decode_roundtrips() {
        let text = "\
# group with a rect + line, plus a top-level arc
group 10 20
  rect 0 0 32 16 4 255 100 50 255 200
  line 0 0 32 16 1.5 0 0 0 255 255
endgroup
arc 50 50 20 0 90 2 10 20 30 255 128
";
        let ops = parse_scene(text).unwrap();
        assert_eq!(ops.len(), 5);
        assert!(matches!(ops[0], SceneOp::GroupBegin { .. }));
        assert!(matches!(ops[3], SceneOp::GroupEnd));

        let payload = encode_scene(&ops).unwrap();
        let back = decode_scene(&payload).unwrap();
        assert_eq!(back, ops);
    }

    #[test]
    fn wrong_arity_is_rejected() {
        assert!(parse_scene("rect 0 0 1 1\n").is_err());
    }

    #[test]
    fn unknown_op_is_rejected() {
        assert!(parse_scene("wiggle 1 2 3\n").is_err());
    }

    #[test]
    fn border_label_blit_roundtrip() {
        let text = "\
border 0 0 64 32 2 4 200 200 200 255 255
label noto-sans 10 20 0 0 0 255 255 hi
blit thumb-1 0 0 16 16
";
        let ops = parse_scene(text).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0], SceneOp::Border { .. }));
        assert!(matches!(ops[1], SceneOp::Label { .. }));
        assert!(matches!(ops[2], SceneOp::Blit { .. }));

        let payload = encode_scene(&ops).unwrap();
        let back = decode_scene(&payload).unwrap();
        assert_eq!(back, ops);
    }

    #[test]
    fn group_opacity_and_disjoint_args_parse() {
        let text = "\
group 10 20
endgroup
group 10 20 128
endgroup
group 10 20 200 disjoint
endgroup
";
        let ops = parse_scene(text).unwrap();
        let mut idx = 0;
        for (expected_opacity, expected_hint) in
            [(None, false), (Some(128u8), false), (Some(200u8), true)]
        {
            match &ops[idx] {
                SceneOp::GroupBegin {
                    opacity,
                    disjoint_hint,
                    ..
                } => {
                    assert_eq!(*opacity, expected_opacity);
                    assert_eq!(*disjoint_hint, expected_hint);
                }
                _ => panic!("expected GroupBegin at {idx}"),
            }
            idx += 2;
        }

        let payload = encode_scene(&ops).unwrap();
        let back = decode_scene(&payload).unwrap();
        assert_eq!(back, ops);
    }

    #[test]
    fn group_rejects_bad_disjoint_token() {
        assert!(parse_scene("group 10 20 128 wat\n").is_err());
    }
}
