//! `path!` and `scene!` — compile-time vector geometry.
//!
//! Numeric literals are folded to 24.8 fixed-point raw values at expansion
//! time (`Fixed::from_f32` is not const), so both macros emit `&'static`
//! slices usable in `const` / `static` with no runtime float work.

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
    quote! { ::mirui::types::Fixed::from_raw(#raw) }
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
                other => {
                    return Err(syn::Error::new(
                        kw.span(),
                        format!(
                            "unknown group option `{other}`; expected translate / rotate / scale / opacity"
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
    })
}

#[derive(Clone, Copy)]
struct GroupOptions {
    m: [i32; 6],
    opacity: Option<u8>,
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

struct PathInput {
    steps: Punctuated<PathStep, Token![;]>,
}

impl Parse for PathInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            steps: Punctuated::parse_terminated(input)?,
        })
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

pub fn expand_path(input: TokenStream) -> TokenStream {
    let parsed = match parse2::<PathInput>(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    let cmds = parsed.steps.iter().map(path_step_tokens);
    quote! {
        {
            const CMDS: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
            CMDS
        }
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
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
    },
    Label {
        token: syn::LitStr,
        x: i32,
        y: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        opa: u8,
        text: syn::LitStr,
    },
    Blit {
        token: syn::LitStr,
        px: i32,
        py: i32,
        sx: i32,
        sy: i32,
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
                let body;
                syn::braced!(body in input);
                let punct: Punctuated<PathStep, Token![;]> = Punctuated::parse_terminated(&body)?;
                SceneStmt::FillPath {
                    steps: punct.into_iter().collect(),
                    r: raw_byte(input)?,
                    g: raw_byte(input)?,
                    b: raw_byte(input)?,
                    a: raw_byte(input)?,
                    opa: raw_byte(input)?,
                }
            }
            "label" => SceneStmt::Label {
                token: input.parse::<syn::LitStr>()?,
                x: input.parse::<Num>()?.0,
                y: input.parse::<Num>()?.0,
                r: raw_byte(input)?,
                g: raw_byte(input)?,
                b: raw_byte(input)?,
                a: raw_byte(input)?,
                opa: raw_byte(input)?,
                text: input.parse::<syn::LitStr>()?,
            },
            "blit" => SceneStmt::Blit {
                token: input.parse::<syn::LitStr>()?,
                px: input.parse::<Num>()?.0,
                py: input.parse::<Num>()?.0,
                sx: input.parse::<Num>()?.0,
                sy: input.parse::<Num>()?.0,
            },
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
            r,
            g,
            b,
            a,
            opa,
        } => {
            let cmds = steps.iter().map(path_step_tokens);
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::FillPath {
                    path: {
                        const P: &[::mirui::render::path::PathCmd] = &[#(#cmds),*];
                        ::mirui::__Cow::Borrowed(P)
                    },
                    transform: ::mirui::types::Transform::IDENTITY,
                    color: #col,
                    opa: #opa,
                    fill_rule: ::mirui::render::raster::FillRule::EvenOdd,
                }
            }
        }
        SceneStmt::Label {
            token,
            x,
            y,
            r,
            g,
            b,
            a,
            opa,
            text,
        } => {
            let pos = point(*x, *y);
            let col = color_tokens(*r, *g, *b, *a);
            quote! {
                ::mirui::render::scene::SceneOp::Label {
                    font: ::mirui::render::scene::ResourceRef::Token(
                        ::mirui::__Cow::Borrowed(#token)
                    ),
                    pos: #pos,
                    transform: ::mirui::types::Transform::IDENTITY,
                    color: #col,
                    opa: #opa,
                    text: ::mirui::__Cow::Borrowed(#text),
                }
            }
        }
        SceneStmt::Blit {
            token,
            px,
            py,
            sx,
            sy,
        } => {
            let pos = point(*px, *py);
            let size = point(*sx, *sy);
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
            quote! {
                ::mirui::render::scene::SceneOp::GroupBegin {
                    transform: ::core::option::Option::Some(
                        ::mirui::types::Transform {
                            m00: #m00, m01: #m01, tx: #tx,
                            m10: #m10, m11: #m11, ty: #ty,
                        }
                    ),
                    opacity: #opacity,
                    clip: ::core::option::Option::None,
                    mask: ::core::option::Option::None,
                    filter: ::core::option::Option::None,
                    disjoint_hint: false,
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
