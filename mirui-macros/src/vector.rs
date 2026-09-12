//! `path!` and `scene!` — compile-time vector geometry.
//!
//! Numeric literals are folded to exact Q24.8 ratios at expansion time, so
//! both macros emit `&'static` slices usable in `const` / `static` with no
//! runtime floating-point work.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, Lit, Token, parse2};

const FRAC_BITS: i32 = 8;

struct Num(i32);

impl Parse for Num {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let neg = input.parse::<Token![-]>().is_ok();
        let lit: Lit = input.parse()?;
        let value: f64 = match &lit {
            Lit::Int(i) => i.base10_parse::<f64>()?,
            Lit::Float(f) => f.base10_parse::<f64>()?,
            other => {
                return Err(syn::Error::new(
                    spanned(other),
                    "expected a numeric literal",
                ));
            }
        };
        let value = if neg { -value } else { value };
        let raw = (value * (1i32 << FRAC_BITS) as f64).round() as i32;
        Ok(Num(raw))
    }
}

fn spanned(lit: &Lit) -> proc_macro2::Span {
    match lit {
        Lit::Str(l) => l.span(),
        Lit::Int(l) => l.span(),
        Lit::Float(l) => l.span(),
        _ => proc_macro2::Span::call_site(),
    }
}

fn raw_byte(input: ParseStream) -> syn::Result<u8> {
    let lit: syn::LitInt = input.parse()?;
    lit.base10_parse::<u8>()
}

fn fixed(raw: i32) -> TokenStream {
    quote! { ::mirui::types::Fixed::from_ratio(#raw, 256) }
}

fn mirx_fixed(raw: i32) -> TokenStream {
    quote! { ::mirx::types::Fixed::from_ratio(#raw, 256) }
}

fn parse_signed_f64(input: ParseStream) -> syn::Result<f64> {
    let neg = input.parse::<Token![-]>().is_ok();
    let lit: Lit = input.parse()?;
    let v = match &lit {
        Lit::Int(i) => i.base10_parse::<f64>()?,
        Lit::Float(f) => f.base10_parse::<f64>()?,
        other => {
            return Err(syn::Error::new(
                spanned(other),
                "expected a numeric literal",
            ));
        }
    };
    Ok(if neg { -v } else { v })
}

type Mat = [f64; 6];
const MAT_ID: Mat = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn mat_compose(a: Mat, b: Mat) -> Mat {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

fn parse_group_options(input: ParseStream) -> syn::Result<GroupOptions> {
    let mut acc = MAT_ID;
    let mut opacity: Option<u8> = None;
    let mut filter: Option<syn::LitStr> = None;
    let mut disjoint = false;
    while !input.is_empty() && !input.peek(Token![;]) {
        if input.peek(Ident) {
            let kw: Ident = input.parse()?;
            match kw.to_string().as_str() {
                "translate" => {
                    let op = [
                        1.0,
                        0.0,
                        parse_signed_f64(input)?,
                        0.0,
                        1.0,
                        parse_signed_f64(input)?,
                    ];
                    acc = mat_compose(acc, op);
                }
                "scale" => {
                    let op = [
                        parse_signed_f64(input)?,
                        0.0,
                        0.0,
                        0.0,
                        parse_signed_f64(input)?,
                        0.0,
                    ];
                    acc = mat_compose(acc, op);
                }
                "rotate" => {
                    let r = parse_signed_f64(input)?.to_radians();
                    let (s, c) = (r.sin(), r.cos());
                    acc = mat_compose(acc, [c, -s, 0.0, s, c, 0.0]);
                }
                "opacity" => {
                    opacity = Some(raw_byte(input)?);
                }
                "filter" => {
                    filter = Some(input.parse::<syn::LitStr>()?);
                }
                "disjoint" => {
                    disjoint = true;
                }
                other => {
                    return Err(syn::Error::new(
                        kw.span(),
                        format!(
                            "unknown group option `{other}`; expected translate / rotate / scale / opacity / filter / disjoint"
                        ),
                    ));
                }
            }
        } else {
            let op = [
                1.0,
                0.0,
                parse_signed_f64(input)?,
                0.0,
                1.0,
                parse_signed_f64(input)?,
            ];
            acc = mat_compose(acc, op);
        }
    }
    Ok(GroupOptions {
        m: acc.map(|v| (v * (1i32 << FRAC_BITS) as f64).round() as i32),
        opacity,
        filter,
        disjoint,
    })
}

#[derive(Clone)]
struct GroupOptions {
    m: [i32; 6],
    opacity: Option<u8>,
    filter: Option<syn::LitStr>,
    disjoint: bool,
}

#[derive(Clone, Copy)]
enum BlitComposite {
    SourceOver,
    Add,
    Screen,
    Multiply,
    Darken,
    Lighten,
    Difference,
}

impl BlitComposite {
    fn from_ident(name: &str) -> Option<Self> {
        Some(match name {
            "source_over" | "source-over" | "sourceover" => Self::SourceOver,
            "add" => Self::Add,
            "screen" => Self::Screen,
            "multiply" => Self::Multiply,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "difference" => Self::Difference,
            _ => return None,
        })
    }

    fn tokens(self) -> TokenStream {
        match self {
            Self::SourceOver => quote!(::mirui::render::command::CompositeMode::SourceOver),
            Self::Add => quote!(::mirui::render::command::CompositeMode::Add),
            Self::Screen => quote!(::mirui::render::command::CompositeMode::Screen),
            Self::Multiply => quote!(::mirui::render::command::CompositeMode::Multiply),
            Self::Darken => quote!(::mirui::render::command::CompositeMode::Darken),
            Self::Lighten => quote!(::mirui::render::command::CompositeMode::Lighten),
            Self::Difference => quote!(::mirui::render::command::CompositeMode::Difference),
        }
    }
}

fn parse_blit_options(input: ParseStream) -> syn::Result<(i32, BlitComposite)> {
    let mut radius = 0;
    let mut composite = BlitComposite::SourceOver;
    while !input.is_empty() && !input.peek(Token![;]) {
        if !input.peek(Ident) {
            break;
        }
        let kw: Ident = input.parse()?;
        match kw.to_string().as_str() {
            "radius" => radius = input.parse::<Num>()?.0,
            "composite" => {
                let mode_id: Ident = input.parse()?;
                composite = BlitComposite::from_ident(&mode_id.to_string()).ok_or_else(|| {
                    syn::Error::new(
                        mode_id.span(),
                        "unknown composite mode; expected source_over / add / screen / multiply / darken / lighten / difference",
                    )
                })?;
            }
            other => {
                return Err(syn::Error::new(
                    kw.span(),
                    format!("unknown blit option `{other}`; expected radius / composite"),
                ));
            }
        }
    }
    Ok((radius, composite))
}

fn point(x: i32, y: i32) -> TokenStream {
    let (x, y) = (fixed(x), fixed(y));
    quote! { ::mirui::types::Point { x: #x, y: #y } }
}

// ---- path! ----

enum PathStep {
    MoveTo(i32, i32),
    LineTo(i32, i32),
    QuadTo(i32, i32, i32, i32),
    CubicTo(i32, i32, i32, i32, i32, i32),
    Close,
}

impl Parse for PathStep {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let cmd: Ident = input.parse()?;
        let step = match cmd.to_string().as_str() {
            "M" => PathStep::MoveTo(input.parse::<Num>()?.0, input.parse::<Num>()?.0),
            "L" => PathStep::LineTo(input.parse::<Num>()?.0, input.parse::<Num>()?.0),
            "Q" => PathStep::QuadTo(
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
            ),
            "C" => PathStep::CubicTo(
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
                input.parse::<Num>()?.0,
            ),
            "Z" => PathStep::Close,
            other => {
                return Err(syn::Error::new(
                    cmd.span(),
                    format!("unknown path command `{other}`; expected M/L/Q/C/Z"),
                ));
            }
        };
        Ok(step)
    }
}

fn path_step_tokens(step: &PathStep) -> TokenStream {
    match step {
        PathStep::MoveTo(x, y) => {
            let p = point(*x, *y);
            quote! { ::mirui::render::path::PathCmd::MoveTo(#p) }
        }
        PathStep::LineTo(x, y) => {
            let p = point(*x, *y);
            quote! { ::mirui::render::path::PathCmd::LineTo(#p) }
        }
        PathStep::QuadTo(cx, cy, ex, ey) => {
            let ctrl = point(*cx, *cy);
            let end = point(*ex, *ey);
            quote! { ::mirui::render::path::PathCmd::QuadTo { ctrl: #ctrl, end: #end } }
        }
        PathStep::CubicTo(c1x, c1y, c2x, c2y, ex, ey) => {
            let c1 = point(*c1x, *c1y);
            let c2 = point(*c2x, *c2y);
            let end = point(*ex, *ey);
            quote! { ::mirui::render::path::PathCmd::CubicTo { ctrl1: #c1, ctrl2: #c2, end: #end } }
        }
        PathStep::Close => quote! { ::mirui::render::path::PathCmd::Close },
    }
}

struct PathMacroInput {
    steps: Vec<PathStep>,
}

impl Parse for PathMacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Two equivalent entries: a token form `M 4 12 L 12 4 Z` for
        // hand-authored constants, and a string-literal form
        // `"M 4 12 L 12 4 Z"` for paths copy-pasted from SVG. Both
        // share the same SvgLexer state machine by funneling the
        // token form through `to_string()` first.
        let span = input.span();
        let src = if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            if !input.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "unexpected token after SVG path string",
                ));
            }
            lit.value()
        } else {
            let stream: proc_macro2::TokenStream = input.parse()?;
            stream.to_string()
        };
        let steps = parse_svg_path_string(&src, span)?;
        Ok(Self { steps })
    }
}

/// State machine per SVG 1.1 §8.3: current point + subpath start (for
/// Z restoring current point). S/T smooth-shorthand state slots are
/// not tracked — D2=B scope skips that shorthand.
fn parse_svg_path_string(svg: &str, span: proc_macro2::Span) -> syn::Result<Vec<PathStep>> {
    let mut lex = SvgLexer { src: svg, pos: 0 };
    let mut steps = Vec::new();
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;
    let mut sub_x = 0.0_f64;
    let mut sub_y = 0.0_f64;
    let mut prev_cmd: Option<char> = None;
    let mut last_was_close = false;

    let to_raw = |v: f64| (v * (1i32 << FRAC_BITS) as f64).round() as i32;

    loop {
        lex.skip_ws();
        if lex.pos >= lex.src.len() {
            break;
        }
        let next = lex.try_cmd();
        let cmd = match (next, prev_cmd) {
            (Some(c), _) => {
                last_was_close = false;
                c
            }
            (None, Some(prev)) => {
                if last_was_close {
                    return Err(svg_err(
                        span,
                        &lex,
                        "number after Z is not allowed; start a new subpath with M/m",
                    ));
                }
                // SVG §8.3.2 implicit repeat: after M/m the next coord
                // pairs are treated as L/l, others repeat themselves.
                match prev {
                    'M' => 'L',
                    'm' => 'l',
                    other => other,
                }
            }
            (None, None) => {
                return Err(svg_err(span, &lex, "expected SVG path command"));
            }
        };
        prev_cmd = Some(cmd);

        match cmd {
            'M' => {
                let (x, y) = (lex.num(span)?, lex.num(span)?);
                cur_x = x;
                cur_y = y;
                sub_x = x;
                sub_y = y;
                steps.push(PathStep::MoveTo(to_raw(x), to_raw(y)));
            }
            'm' => {
                let (dx, dy) = (lex.num(span)?, lex.num(span)?);
                cur_x += dx;
                cur_y += dy;
                sub_x = cur_x;
                sub_y = cur_y;
                steps.push(PathStep::MoveTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'L' => {
                let (x, y) = (lex.num(span)?, lex.num(span)?);
                cur_x = x;
                cur_y = y;
                steps.push(PathStep::LineTo(to_raw(x), to_raw(y)));
            }
            'l' => {
                let (dx, dy) = (lex.num(span)?, lex.num(span)?);
                cur_x += dx;
                cur_y += dy;
                steps.push(PathStep::LineTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'H' => {
                cur_x = lex.num(span)?;
                steps.push(PathStep::LineTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'h' => {
                cur_x += lex.num(span)?;
                steps.push(PathStep::LineTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'V' => {
                cur_y = lex.num(span)?;
                steps.push(PathStep::LineTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'v' => {
                cur_y += lex.num(span)?;
                steps.push(PathStep::LineTo(to_raw(cur_x), to_raw(cur_y)));
            }
            'Q' => {
                let (cx, cy) = (lex.num(span)?, lex.num(span)?);
                let (x, y) = (lex.num(span)?, lex.num(span)?);
                cur_x = x;
                cur_y = y;
                steps.push(PathStep::QuadTo(
                    to_raw(cx),
                    to_raw(cy),
                    to_raw(x),
                    to_raw(y),
                ));
            }
            'q' => {
                let (dcx, dcy) = (lex.num(span)?, lex.num(span)?);
                let (dx, dy) = (lex.num(span)?, lex.num(span)?);
                let (cx, cy) = (cur_x + dcx, cur_y + dcy);
                cur_x += dx;
                cur_y += dy;
                steps.push(PathStep::QuadTo(
                    to_raw(cx),
                    to_raw(cy),
                    to_raw(cur_x),
                    to_raw(cur_y),
                ));
            }
            'C' => {
                let (c1x, c1y) = (lex.num(span)?, lex.num(span)?);
                let (c2x, c2y) = (lex.num(span)?, lex.num(span)?);
                let (x, y) = (lex.num(span)?, lex.num(span)?);
                cur_x = x;
                cur_y = y;
                steps.push(PathStep::CubicTo(
                    to_raw(c1x),
                    to_raw(c1y),
                    to_raw(c2x),
                    to_raw(c2y),
                    to_raw(x),
                    to_raw(y),
                ));
            }
            'c' => {
                let (dc1x, dc1y) = (lex.num(span)?, lex.num(span)?);
                let (dc2x, dc2y) = (lex.num(span)?, lex.num(span)?);
                let (dx, dy) = (lex.num(span)?, lex.num(span)?);
                let (c1x, c1y) = (cur_x + dc1x, cur_y + dc1y);
                let (c2x, c2y) = (cur_x + dc2x, cur_y + dc2y);
                cur_x += dx;
                cur_y += dy;
                steps.push(PathStep::CubicTo(
                    to_raw(c1x),
                    to_raw(c1y),
                    to_raw(c2x),
                    to_raw(c2y),
                    to_raw(cur_x),
                    to_raw(cur_y),
                ));
            }
            'Z' | 'z' => {
                cur_x = sub_x;
                cur_y = sub_y;
                steps.push(PathStep::Close);
                last_was_close = true;
            }
            'A' | 'a' => {
                let rx = lex.num(span)?;
                let ry = lex.num(span)?;
                let phi_deg = lex.num(span)?;
                let large = lex.flag(span)?;
                let sweep = lex.flag(span)?;
                let (ex, ey) = (lex.num(span)?, lex.num(span)?);
                let (end_x, end_y) = if cmd == 'A' {
                    (ex, ey)
                } else {
                    (cur_x + ex, cur_y + ey)
                };
                emit_arc(
                    &mut steps, cur_x, cur_y, end_x, end_y, rx, ry, phi_deg, large, sweep, to_raw,
                );
                cur_x = end_x;
                cur_y = end_y;
            }
            other => {
                return Err(syn::Error::new(
                    span,
                    format!(
                        "unknown SVG path command `{other}` at offset {}",
                        lex.pos.saturating_sub(1)
                    ),
                ));
            }
        }
    }
    Ok(steps)
}

struct SvgLexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> SvgLexer<'a> {
    fn skip_ws(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            if b.is_ascii_whitespace() || b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn try_cmd(&mut self) -> Option<char> {
        self.skip_ws();
        let bytes = self.src.as_bytes();
        if self.pos < bytes.len() && bytes[self.pos].is_ascii_alphabetic() {
            let c = bytes[self.pos] as char;
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn num(&mut self, span: proc_macro2::Span) -> syn::Result<f64> {
        self.skip_ws();
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut i = start;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
            // bare-token form stringifies a unary minus as "- 2"; skip
            // the whitespace so we still see a number after the sign.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        let int_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let had_int_digits = i > int_start;
        let mut had_frac_digits = false;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let frac_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            had_frac_digits = i > frac_start;
        }
        if !had_int_digits && !had_frac_digits {
            return Err(svg_err(span, self, "expected number"));
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        let slice = &self.src[start..i];
        self.pos = i;
        slice
            .parse::<f64>()
            .map_err(|_| svg_err_with_offset(span, self, start, &format!("bad number `{slice}`")))
    }

    fn flag(&mut self, span: proc_macro2::Span) -> syn::Result<bool> {
        self.skip_ws();
        let bytes = self.src.as_bytes();
        if self.pos < bytes.len() {
            let b = bytes[self.pos];
            if b == b'0' || b == b'1' {
                self.pos += 1;
                return Ok(b == b'1');
            }
        }
        Err(svg_err(span, self, "expected arc flag (0 or 1)"))
    }
}

/// SVG 1.1 Appendix B.2.4 endpoint → center conversion + cubic-Bezier
/// approximation, ≤90° per segment with handle = (4/3)·tan(θ/4).
/// Path::arc proves the math at 0.027% / segment.
#[allow(clippy::too_many_arguments)]
fn emit_arc(
    steps: &mut Vec<PathStep>,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    mut rx: f64,
    mut ry: f64,
    phi_deg: f64,
    large: bool,
    sweep: bool,
    to_raw: impl Fn(f64) -> i32,
) {
    rx = rx.abs();
    ry = ry.abs();
    if rx < 1e-9 || ry < 1e-9 || (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        steps.push(PathStep::LineTo(to_raw(x2), to_raw(y2)));
        return;
    }
    let phi = phi_deg.to_radians();
    let (cos_p, sin_p) = (phi.cos(), phi.sin());

    let dx = (x1 - x2) * 0.5;
    let dy = (y1 - y2) * 0.5;
    let x1p = cos_p * dx + sin_p * dy;
    let y1p = -sin_p * dx + cos_p * dy;

    let mut lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
        lam = 1.0;
    }
    let num = (1.0 - lam).max(0.0) * (rx * rx) * (ry * ry);
    let denom = (rx * rx) * (y1p * y1p) + (ry * ry) * (x1p * x1p);
    let mut coef = if denom > 0.0 {
        (num / denom).sqrt()
    } else {
        0.0
    };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * (rx * y1p) / ry;
    let cyp = -coef * (ry * x1p) / rx;

    let cx = cos_p * cxp - sin_p * cyp + (x1 + x2) * 0.5;
    let cy = sin_p * cxp + cos_p * cyp + (y1 + y2) * 0.5;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let n = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let dot = (ux * vx + uy * vy) / n;
        let mut a = dot.clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= core::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += core::f64::consts::TAU;
    }

    let segs = ((delta.abs() / core::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = delta / segs as f64;
    let k = (4.0 / 3.0) * (step / 4.0).tan();

    let mut a = theta1;
    for _ in 0..segs {
        let a_next = a + step;
        let (sa, ca) = a.sin_cos();
        let (sb, cb) = a_next.sin_cos();
        let p0x_unit = ca;
        let p0y_unit = sa;
        let p3x_unit = cb;
        let p3y_unit = sb;
        let p1x_unit = ca - k * sa;
        let p1y_unit = sa + k * ca;
        let p2x_unit = cb + k * sb;
        let p2y_unit = sb - k * cb;
        let map = |ux: f64, uy: f64| -> (f64, f64) {
            let px = ux * rx;
            let py = uy * ry;
            (cos_p * px - sin_p * py + cx, sin_p * px + cos_p * py + cy)
        };
        let (_, _) = map(p0x_unit, p0y_unit); // p0 already emitted via previous cur/MoveTo
        let (c1x, c1y) = map(p1x_unit, p1y_unit);
        let (c2x, c2y) = map(p2x_unit, p2y_unit);
        let (ex, ey) = map(p3x_unit, p3y_unit);
        steps.push(PathStep::CubicTo(
            to_raw(c1x),
            to_raw(c1y),
            to_raw(c2x),
            to_raw(c2y),
            to_raw(ex),
            to_raw(ey),
        ));
        a = a_next;
    }
}

fn svg_err(span: proc_macro2::Span, lex: &SvgLexer, msg: &str) -> syn::Error {
    svg_err_with_offset(span, lex, lex.pos, msg)
}

fn svg_err_with_offset(
    span: proc_macro2::Span,
    lex: &SvgLexer,
    at: usize,
    msg: &str,
) -> syn::Error {
    let win_start = at.saturating_sub(10);
    let win_end = (at + 10).min(lex.src.len());
    let ctx = &lex.src[win_start..win_end];
    syn::Error::new(span, format!("svg path: {msg} at offset {at} near `{ctx}`"))
}

pub fn expand_path(input: TokenStream) -> TokenStream {
    let parsed = match parse2::<PathMacroInput>(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    let cmds = parsed.steps.iter().map(path_step_tokens);
    quote! {
        ::mirui::render::path::Path::from_static({
            const CMDS: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
            CMDS
        })
    }
}

// ---- scene! ----

enum SceneStmt {
    Rect {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
    },
    Border {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        width: i32,
        radius: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        width: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
    },
    Arc {
        cx: i32,
        cy: i32,
        radius: i32,
        start: i32,
        end: i32,
        width: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
    },
    FillPath {
        steps: Vec<PathStep>,
        paint: PaintExpr,
        opa: u8,
        fill_rule: FillRuleNode,
    },
    StrokePath {
        steps: Vec<PathStep>,
        width: i32,
        paint: PaintExpr,
        opa: u8,
        cap: LineCapNode,
        join: LineJoinNode,
        miter_limit: i32,
        dash: Vec<i32>,
    },
    PushClip {
        steps: Vec<PathStep>,
        fill_rule: FillRuleNode,
    },
    PopClip,
    Blit {
        token: syn::LitStr,
        px: i32,
        py: i32,
        sx: i32,
        sy: i32,
        radius: i32,
        composite: BlitComposite,
    },
    Group {
        opts: GroupOptions,
    },
    EndGroup,
}

impl Parse for SceneStmt {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind: Ident = input.parse()?;
        let stmt = match kind.to_string().as_str() {
            "rect" => SceneStmt::Rect {
                x: input.parse::<Num>()?.0,
                y: input.parse::<Num>()?.0,
                w: input.parse::<Num>()?.0,
                h: input.parse::<Num>()?.0,
                radius: input.parse::<Num>()?.0,
                r: raw_byte(input)?,
                g: raw_byte(input)?,
                b: raw_byte(input)?,
                a: raw_byte(input)?,
                opa: raw_byte(input)?,
            },
            "line" => SceneStmt::Line {
                x1: input.parse::<Num>()?.0,
                y1: input.parse::<Num>()?.0,
                x2: input.parse::<Num>()?.0,
                y2: input.parse::<Num>()?.0,
                width: input.parse::<Num>()?.0,
                r: raw_byte(input)?,
                g: raw_byte(input)?,
                b: raw_byte(input)?,
                a: raw_byte(input)?,
                opa: raw_byte(input)?,
            },
            "arc" => SceneStmt::Arc {
                cx: input.parse::<Num>()?.0,
                cy: input.parse::<Num>()?.0,
                radius: input.parse::<Num>()?.0,
                start: input.parse::<Num>()?.0,
                end: input.parse::<Num>()?.0,
                width: input.parse::<Num>()?.0,
                r: raw_byte(input)?,
                g: raw_byte(input)?,
                b: raw_byte(input)?,
                a: raw_byte(input)?,
                opa: raw_byte(input)?,
            },
            "border" => SceneStmt::Border {
                x: input.parse::<Num>()?.0,
                y: input.parse::<Num>()?.0,
                w: input.parse::<Num>()?.0,
                h: input.parse::<Num>()?.0,
                width: input.parse::<Num>()?.0,
                radius: input.parse::<Num>()?.0,
                r: raw_byte(input)?,
                g: raw_byte(input)?,
                b: raw_byte(input)?,
                a: raw_byte(input)?,
                opa: raw_byte(input)?,
            },
            "fill_path" => {
                let steps = parse_path_steps_block(input)?;
                let paint = parse_paint_expr(input)?;
                let opa = raw_byte(input)?;
                let mut fill_rule = FillRuleNode::EvenOdd;
                while !input.is_empty() && !input.peek(Token![;]) {
                    let k: Ident = input.parse()?;
                    match k.to_string().as_str() {
                        "fill_rule" => fill_rule = parse_fill_rule(input)?,
                        other => {
                            return Err(syn::Error::new(
                                k.span(),
                                format!("unknown fill_path option `{other}`; expected fill_rule"),
                            ));
                        }
                    }
                }
                SceneStmt::FillPath {
                    steps,
                    paint,
                    opa,
                    fill_rule,
                }
            }
            "stroke_path" => {
                let steps = parse_path_steps_block(input)?;
                let width = input.parse::<Num>()?.0;
                let paint = parse_paint_expr(input)?;
                let opa = raw_byte(input)?;
                let mut cap = LineCapNode::Butt;
                let mut join = LineJoinNode::Miter;
                let mut miter_limit: i32 = 4 << FRAC_BITS;
                let mut dash: Vec<i32> = Vec::new();
                while !input.is_empty() && !input.peek(Token![;]) {
                    let k: Ident = input.parse()?;
                    match k.to_string().as_str() {
                        "cap" => cap = parse_line_cap(input)?,
                        "join" => join = parse_line_join(input)?,
                        "miter" => miter_limit = input.parse::<Num>()?.0,
                        "dash" => {
                            let d;
                            syn::bracketed!(d in input);
                            while !d.is_empty() {
                                dash.push(d.parse::<Num>()?.0);
                            }
                        }
                        other => {
                            return Err(syn::Error::new(
                                k.span(),
                                format!(
                                    "unknown stroke_path option `{other}`; expected cap / join / miter / dash"
                                ),
                            ));
                        }
                    }
                }
                SceneStmt::StrokePath {
                    steps,
                    width,
                    paint,
                    opa,
                    cap,
                    join,
                    miter_limit,
                    dash,
                }
            }
            "push_clip" => {
                let steps = parse_path_steps_block(input)?;
                let mut fill_rule = FillRuleNode::EvenOdd;
                while !input.is_empty() && !input.peek(Token![;]) {
                    let k: Ident = input.parse()?;
                    match k.to_string().as_str() {
                        "fill_rule" => fill_rule = parse_fill_rule(input)?,
                        other => {
                            return Err(syn::Error::new(
                                k.span(),
                                format!("unknown push_clip option `{other}`; expected fill_rule"),
                            ));
                        }
                    }
                }
                SceneStmt::PushClip { steps, fill_rule }
            }
            "pop_clip" => SceneStmt::PopClip,
            "blit" => {
                let token = input.parse::<syn::LitStr>()?;
                let px = input.parse::<Num>()?.0;
                let py = input.parse::<Num>()?.0;
                let sx = input.parse::<Num>()?.0;
                let sy = input.parse::<Num>()?.0;
                let (radius, composite) = parse_blit_options(input)?;
                SceneStmt::Blit {
                    token,
                    px,
                    py,
                    sx,
                    sy,
                    radius,
                    composite,
                }
            }
            "group" => SceneStmt::Group {
                opts: parse_group_options(input)?,
            },
            "endgroup" => SceneStmt::EndGroup,
            other => {
                return Err(syn::Error::new(
                    kind.span(),
                    format!("unknown scene op `{other}`"),
                ));
            }
        };
        Ok(stmt)
    }
}

struct SceneInput {
    stmts: Punctuated<SceneStmt, Token![;]>,
}

impl Parse for SceneInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            stmts: Punctuated::parse_terminated(input)?,
        })
    }
}

fn color_tokens(r: u8, g: u8, b: u8, a: u8) -> TokenStream {
    quote! { ::mirui::types::Color { r: #r, g: #g, b: #b, a: #a } }
}

fn mirx_color_tokens(r: u8, g: u8, b: u8, a: u8) -> TokenStream {
    quote! { ::mirui::render::scene::MirxColor { r: #r, g: #g, b: #b, a: #a } }
}

// ---- Paint expression (solid color or gradient) ----

enum PaintExpr {
    Solid(u8, u8, u8, u8),
    LinearGradient {
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        stops: Vec<(f64, u8, u8, u8, u8)>,
        spread: SpreadModeNode,
        units: GradientUnitsNode,
    },
    RadialGradient {
        cx: f64,
        cy: f64,
        r: f64,
        fx: f64,
        fy: f64,
        fr: f64,
        stops: Vec<(f64, u8, u8, u8, u8)>,
        spread: SpreadModeNode,
        units: GradientUnitsNode,
    },
}

#[derive(Clone, Copy)]
enum SpreadModeNode {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy)]
enum GradientUnitsNode {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

fn parse_f64(input: ParseStream) -> syn::Result<f64> {
    parse_signed_f64(input)
}

fn parse_rgba(input: ParseStream) -> syn::Result<(u8, u8, u8, u8)> {
    Ok((
        raw_byte(input)?,
        raw_byte(input)?,
        raw_byte(input)?,
        raw_byte(input)?,
    ))
}

#[allow(clippy::type_complexity)]
fn parse_gradient_stops(input: ParseStream) -> syn::Result<Vec<(f64, u8, u8, u8, u8)>> {
    let mut stops = Vec::new();
    while !input.is_empty() && !input.peek(syn::token::Bracket) {
        let offset = parse_f64(input)?;
        let (r, g, b, a) = parse_rgba(input)?;
        stops.push((offset, r, g, b, a));
    }
    Ok(stops)
}

fn parse_spread_mode(input: ParseStream) -> syn::Result<SpreadModeNode> {
    let kw: Ident = input.parse()?;
    match kw.to_string().as_str() {
        "pad" => Ok(SpreadModeNode::Pad),
        "reflect" => Ok(SpreadModeNode::Reflect),
        "repeat" => Ok(SpreadModeNode::Repeat),
        other => Err(syn::Error::new(
            kw.span(),
            format!("unknown spread mode `{other}`; expected pad / reflect / repeat"),
        )),
    }
}

fn parse_gradient_units(input: ParseStream) -> syn::Result<GradientUnitsNode> {
    let kw: Ident = input.parse()?;
    match kw.to_string().as_str() {
        "user_space" => Ok(GradientUnitsNode::UserSpaceOnUse),
        "object_bbox" => Ok(GradientUnitsNode::ObjectBoundingBox),
        other => Err(syn::Error::new(
            kw.span(),
            format!("unknown gradient units `{other}`; expected user_space / object_bbox"),
        )),
    }
}

fn parse_paint_expr(input: ParseStream) -> syn::Result<PaintExpr> {
    if input.peek(syn::token::Brace) {
        let body;
        syn::braced!(body in input);
        let kw: Ident = body.parse()?;
        match kw.to_string().as_str() {
            "linear" => {
                let sx = parse_f64(&body)?;
                let sy = parse_f64(&body)?;
                let ex = parse_f64(&body)?;
                let ey = parse_f64(&body)?;
                let mut stops = Vec::new();
                let mut spread = SpreadModeNode::Pad;
                let mut units = GradientUnitsNode::ObjectBoundingBox;
                while !body.is_empty() {
                    let k: Ident = body.parse()?;
                    match k.to_string().as_str() {
                        "stops" => {
                            let s;
                            syn::bracketed!(s in body);
                            stops = parse_gradient_stops(&s)?;
                        }
                        "spread" => spread = parse_spread_mode(&body)?,
                        "units" => units = parse_gradient_units(&body)?,
                        other => {
                            return Err(syn::Error::new(
                                k.span(),
                                format!(
                                    "unknown linear gradient option `{other}`; expected stops / spread / units"
                                ),
                            ));
                        }
                    }
                }
                Ok(PaintExpr::LinearGradient {
                    start_x: sx,
                    start_y: sy,
                    end_x: ex,
                    end_y: ey,
                    stops,
                    spread,
                    units,
                })
            }
            "radial" => {
                let cx = parse_f64(&body)?;
                let cy = parse_f64(&body)?;
                let r = parse_f64(&body)?;
                let fx = parse_f64(&body)?;
                let fy = parse_f64(&body)?;
                let fr = parse_f64(&body)?;
                let mut stops = Vec::new();
                let mut spread = SpreadModeNode::Pad;
                let mut units = GradientUnitsNode::ObjectBoundingBox;
                while !body.is_empty() {
                    let k: Ident = body.parse()?;
                    match k.to_string().as_str() {
                        "stops" => {
                            let s;
                            syn::bracketed!(s in body);
                            stops = parse_gradient_stops(&s)?;
                        }
                        "spread" => spread = parse_spread_mode(&body)?,
                        "units" => units = parse_gradient_units(&body)?,
                        other => {
                            return Err(syn::Error::new(
                                k.span(),
                                format!(
                                    "unknown radial gradient option `{other}`; expected stops / spread / units"
                                ),
                            ));
                        }
                    }
                }
                Ok(PaintExpr::RadialGradient {
                    cx,
                    cy,
                    r,
                    fx,
                    fy,
                    fr,
                    stops,
                    spread,
                    units,
                })
            }
            other => Err(syn::Error::new(
                kw.span(),
                format!("unknown paint kind `{other}`; expected linear / radial"),
            )),
        }
    } else {
        let (r, g, b, a) = parse_rgba(input)?;
        Ok(PaintExpr::Solid(r, g, b, a))
    }
}

fn spread_tokens(s: SpreadModeNode) -> TokenStream {
    match s {
        SpreadModeNode::Pad => quote! { ::mirui::render::scene::SpreadMode::Pad },
        SpreadModeNode::Reflect => quote! { ::mirui::render::scene::SpreadMode::Reflect },
        SpreadModeNode::Repeat => quote! { ::mirui::render::scene::SpreadMode::Repeat },
    }
}

fn units_tokens(u: GradientUnitsNode) -> TokenStream {
    match u {
        GradientUnitsNode::UserSpaceOnUse => {
            quote! { ::mirui::render::scene::GradientUnits::UserSpaceOnUse }
        }
        GradientUnitsNode::ObjectBoundingBox => {
            quote! { ::mirui::render::scene::GradientUnits::ObjectBoundingBox }
        }
    }
}

fn stop_tokens(stops: &[(f64, u8, u8, u8, u8)]) -> Vec<TokenStream> {
    stops
        .iter()
        .map(|(off, r, g, b, a)| {
            let off_raw = (*off * (1i32 << FRAC_BITS) as f64).round() as i32;
            quote! {
                ::mirui::render::scene::GradientStop {
                    offset: ::mirx::types::Fixed::from_ratio(#off_raw, 256),
                    color: ::mirui::render::scene::MirxColor { r: #r, g: #g, b: #b, a: #a },
                }
            }
        })
        .collect()
}

fn paint_expr_tokens(p: &PaintExpr) -> TokenStream {
    match p {
        PaintExpr::Solid(r, g, b, a) => {
            let col = mirx_color_tokens(*r, *g, *b, *a);
            quote! { ::mirui::render::scene::Paint::Color(#col) }
        }
        PaintExpr::LinearGradient {
            start_x,
            start_y,
            end_x,
            end_y,
            stops,
            spread,
            units,
        } => {
            let sx = mirx_fixed((*start_x * (1i32 << FRAC_BITS) as f64).round() as i32);
            let sy = mirx_fixed((*start_y * (1i32 << FRAC_BITS) as f64).round() as i32);
            let ex = mirx_fixed((*end_x * (1i32 << FRAC_BITS) as f64).round() as i32);
            let ey = mirx_fixed((*end_y * (1i32 << FRAC_BITS) as f64).round() as i32);
            let stops_t = stop_tokens(stops);
            let spread_t = spread_tokens(*spread);
            let units_t = units_tokens(*units);
            quote! {
                ::mirui::render::scene::Paint::LinearGradient(
                    ::mirui::render::scene::LinearGradient {
                        start: ::mirx::types::Point { x: #sx, y: #sy },
                        end: ::mirx::types::Point { x: #ex, y: #ey },
                        stops: ::mirui::__Cow::Borrowed(&[#(#stops_t),*]),
                        spread: #spread_t,
                        units: #units_t,
                        transform: ::mirx::types::Transform::IDENTITY,
                    }
                )
            }
        }
        PaintExpr::RadialGradient {
            cx,
            cy,
            r,
            fx,
            fy,
            fr,
            stops,
            spread,
            units,
        } => {
            let cx_t = mirx_fixed((*cx * (1i32 << FRAC_BITS) as f64).round() as i32);
            let cy_t = mirx_fixed((*cy * (1i32 << FRAC_BITS) as f64).round() as i32);
            let r_t = mirx_fixed((*r * (1i32 << FRAC_BITS) as f64).round() as i32);
            let fx_t = mirx_fixed((*fx * (1i32 << FRAC_BITS) as f64).round() as i32);
            let fy_t = mirx_fixed((*fy * (1i32 << FRAC_BITS) as f64).round() as i32);
            let fr_t = mirx_fixed((*fr * (1i32 << FRAC_BITS) as f64).round() as i32);
            let stops_t = stop_tokens(stops);
            let spread_t = spread_tokens(*spread);
            let units_t = units_tokens(*units);
            quote! {
                ::mirui::render::scene::Paint::RadialGradient(
                    ::mirui::render::scene::RadialGradient {
                        center: ::mirx::types::Point { x: #cx_t, y: #cy_t },
                        radius: #r_t,
                        focal: ::mirx::types::Point { x: #fx_t, y: #fy_t },
                        focal_radius: #fr_t,
                        stops: ::mirui::__Cow::Borrowed(&[#(#stops_t),*]),
                        spread: #spread_t,
                        units: #units_t,
                        transform: ::mirx::types::Transform::IDENTITY,
                    }
                )
            }
        }
    }
}

#[derive(Clone, Copy)]
enum FillRuleNode {
    EvenOdd,
    NonZero,
}

#[derive(Clone, Copy)]
enum LineCapNode {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy)]
enum LineJoinNode {
    Miter,
    Round,
    Bevel,
}

fn parse_fill_rule(input: ParseStream) -> syn::Result<FillRuleNode> {
    let kw: Ident = input.parse()?;
    match kw.to_string().as_str() {
        "evenodd" => Ok(FillRuleNode::EvenOdd),
        "nonzero" => Ok(FillRuleNode::NonZero),
        other => Err(syn::Error::new(
            kw.span(),
            format!("unknown fill rule `{other}`; expected evenodd / nonzero"),
        )),
    }
}

fn parse_line_cap(input: ParseStream) -> syn::Result<LineCapNode> {
    let kw: Ident = input.parse()?;
    match kw.to_string().as_str() {
        "butt" => Ok(LineCapNode::Butt),
        "round" => Ok(LineCapNode::Round),
        "square" => Ok(LineCapNode::Square),
        other => Err(syn::Error::new(
            kw.span(),
            format!("unknown line cap `{other}`; expected butt / round / square"),
        )),
    }
}

fn parse_line_join(input: ParseStream) -> syn::Result<LineJoinNode> {
    let kw: Ident = input.parse()?;
    match kw.to_string().as_str() {
        "miter" => Ok(LineJoinNode::Miter),
        "round" => Ok(LineJoinNode::Round),
        "bevel" => Ok(LineJoinNode::Bevel),
        other => Err(syn::Error::new(
            kw.span(),
            format!("unknown line join `{other}`; expected miter / round / bevel"),
        )),
    }
}

fn fill_rule_tokens(f: FillRuleNode) -> TokenStream {
    match f {
        FillRuleNode::EvenOdd => {
            quote! { ::mirui::render::raster::FillRule::EvenOdd }
        }
        FillRuleNode::NonZero => {
            quote! { ::mirui::render::raster::FillRule::NonZero }
        }
    }
}

fn line_cap_tokens(c: LineCapNode) -> TokenStream {
    match c {
        LineCapNode::Butt => quote! { ::mirui::render::scene::LineCap::Butt },
        LineCapNode::Round => quote! { ::mirui::render::scene::LineCap::Round },
        LineCapNode::Square => quote! { ::mirui::render::scene::LineCap::Square },
    }
}

fn line_join_tokens(j: LineJoinNode) -> TokenStream {
    match j {
        LineJoinNode::Miter => quote! { ::mirui::render::scene::LineJoin::Miter },
        LineJoinNode::Round => quote! { ::mirui::render::scene::LineJoin::Round },
        LineJoinNode::Bevel => quote! { ::mirui::render::scene::LineJoin::Bevel },
    }
}

fn parse_path_steps_block(input: ParseStream) -> syn::Result<Vec<PathStep>> {
    let body;
    syn::braced!(body in input);
    let punct: Punctuated<PathStep, Token![;]> = Punctuated::parse_terminated(&body)?;
    Ok(punct.into_iter().collect())
}

fn scene_stmt_tokens(stmt: &SceneStmt) -> TokenStream {
    match stmt {
        SceneStmt::Rect {
            x,
            y,
            w,
            h,
            radius,
            r,
            g,
            b,
            a,
            opa,
        } => {
            let (fx, fy, fw, fh) = (fixed(*x), fixed(*y), fixed(*w), fixed(*h));
            let rad = fixed(*radius);
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::FillRect {
                    area: ::mirui::types::Rect { x: #fx, y: #fy, w: #fw, h: #fh },
                    transform: ::mirui::types::Transform::IDENTITY,
                    quad: ::core::option::Option::None,
                    color: #col,
                    radius: #rad,
                    opa: #opa,
                }
            }
        }
        SceneStmt::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            r,
            g,
            b,
            a,
            opa,
        } => {
            let p1 = point(*x1, *y1);
            let p2 = point(*x2, *y2);
            let w = fixed(*width);
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::Line {
                    p1: #p1,
                    p2: #p2,
                    transform: ::mirui::types::Transform::IDENTITY,
                    color: #col,
                    width: #w,
                    opa: #opa,
                }
            }
        }
        SceneStmt::Arc {
            cx,
            cy,
            radius,
            start,
            end,
            width,
            r,
            g,
            b,
            a,
            opa,
        } => {
            let center = point(*cx, *cy);
            let (rad, st, en, w) = (fixed(*radius), fixed(*start), fixed(*end), fixed(*width));
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::Arc {
                    center: #center,
                    transform: ::mirui::types::Transform::IDENTITY,
                    radius: #rad,
                    start_angle: #st,
                    end_angle: #en,
                    color: #col,
                    width: #w,
                    opa: #opa,
                }
            }
        }
        SceneStmt::Border {
            x,
            y,
            w,
            h,
            width,
            radius,
            r,
            g,
            b,
            a,
            opa,
        } => {
            let (fx, fy, fw, fh) = (fixed(*x), fixed(*y), fixed(*w), fixed(*h));
            let (wd, rad) = (fixed(*width), fixed(*radius));
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::Border {
                    area: ::mirui::types::Rect { x: #fx, y: #fy, w: #fw, h: #fh },
                    transform: ::mirui::types::Transform::IDENTITY,
                    quad: ::core::option::Option::None,
                    color: #col,
                    width: #wd,
                    radius: #rad,
                    opa: #opa,
                }
            }
        }
        SceneStmt::FillPath {
            steps,
            paint,
            opa,
            fill_rule,
        } => {
            let cmds = steps.iter().map(path_step_tokens);
            let paint_t = paint_expr_tokens(paint);
            let fr = fill_rule_tokens(*fill_rule);
            quote! {
                ::mirui::render::scene::SceneOp::FillPath {
                    path: ::mirui::render::path::Path::from_static({
                        const P: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
                        P
                    }),
                    transform: ::mirui::types::Transform::IDENTITY,
                    paint: #paint_t,
                    opa: #opa,
                    fill_rule: #fr,
                }
            }
        }
        SceneStmt::StrokePath {
            steps,
            width,
            paint,
            opa,
            cap,
            join,
            miter_limit,
            dash,
        } => {
            let cmds = steps.iter().map(path_step_tokens);
            let paint_t = paint_expr_tokens(paint);
            let w = fixed(*width);
            let cap_t = line_cap_tokens(*cap);
            let join_t = line_join_tokens(*join);
            let ml = fixed(*miter_limit);
            let dash_t: Vec<_> = dash.iter().map(|d| fixed(*d)).collect();
            quote! {
                ::mirui::render::scene::SceneOp::StrokePath {
                    path: ::mirui::render::path::Path::from_static({
                        const P: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
                        P
                    }),
                    transform: ::mirui::types::Transform::IDENTITY,
                    paint: #paint_t,
                    width: #w,
                    opa: #opa,
                    line_cap: #cap_t,
                    line_join: #join_t,
                    miter_limit: #ml,
                    dash: ::mirui::__Cow::Borrowed(&[#(#dash_t),*]),
                }
            }
        }
        SceneStmt::PushClip { steps, fill_rule } => {
            let cmds = steps.iter().map(path_step_tokens);
            let fr = fill_rule_tokens(*fill_rule);
            quote! {
                ::mirui::render::scene::SceneOp::PushClip {
                    path: ::mirui::render::path::Path::from_static({
                        const P: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
                        P
                    }),
                    transform: ::mirui::types::Transform::IDENTITY,
                    fill_rule: #fr,
                }
            }
        }
        SceneStmt::PopClip => quote! { ::mirui::render::scene::SceneOp::PopClip },
        SceneStmt::Blit {
            token,
            px,
            py,
            sx,
            sy,
            radius,
            composite,
        } => {
            let pos = point(*px, *py);
            let size = point(*sx, *sy);
            let rad = fixed(*radius);
            let comp = composite.tokens();
            quote! {
                ::mirui::render::scene::SceneOp::Blit {
                    texture: ::mirui::render::scene::ResourceRef::Token(
                        ::mirui::__Cow::Borrowed(#token)
                    ),
                    pos: #pos,
                    size: #size,
                    transform: ::mirui::types::Transform::IDENTITY,
                    quad: ::core::option::Option::None,
                    opa: 255,
                    radius: #rad,
                    composite: #comp,
                }
            }
        }
        SceneStmt::Group { opts } => {
            let m = opts.m;
            let [m00, m01, tx, m10, m11, ty] = [
                fixed(m[0]),
                fixed(m[1]),
                fixed(m[2]),
                fixed(m[3]),
                fixed(m[4]),
                fixed(m[5]),
            ];
            let opacity = match opts.opacity {
                Some(n) => quote! { ::core::option::Option::Some(#n) },
                None => quote! { ::core::option::Option::None },
            };
            let filter = match &opts.filter {
                Some(lit) => quote! {
                    ::core::option::Option::Some(::mirui::render::scene::ResourceRef::Token(
                        ::mirui::__Cow::Borrowed(#lit)
                    ))
                },
                None => quote! { ::core::option::Option::None },
            };
            let disjoint = opts.disjoint;
            quote! {
                ::mirui::render::scene::SceneOp::GroupBegin {
                    transform: ::core::option::Option::Some(
                        ::mirui::types::Transform {
                            m00: #m00, m01: #m01, tx: #tx,
                            m10: #m10, m11: #m11, ty: #ty,
                        }
                    ),
                    projective: ::core::option::Option::None,
                    opacity: #opacity,
                    clip: ::core::option::Option::None,
                    mask: ::core::option::Option::None,
                    filter: #filter,
                    disjoint_hint: #disjoint,
                }
            }
        }
        SceneStmt::EndGroup => quote! { ::mirui::render::scene::SceneOp::GroupEnd },
    }
}

pub fn expand_scene(input: TokenStream) -> TokenStream {
    let parsed = match parse2::<SceneInput>(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    let ops = parsed.stmts.iter().map(scene_stmt_tokens);
    quote! {
        {
            const OPS: &[::mirui::render::scene::SceneOp] = &[#(#ops),*];
            OPS
        }
    }
}
