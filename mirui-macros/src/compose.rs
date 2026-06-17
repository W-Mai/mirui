use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::{Ident, Result, Token, Visibility, braced};

/// Canvas method table. Each entry = `(name, param_list, is_default_impl)`.
/// `is_default_impl = true` means the trait has a default impl, so the macro
/// only emits a forwarder when the user explicitly routes it.
///
/// Signatures must stay in sync with `src/draw/backend.rs` — the integration
/// test `compose_backend_dispatch` covers every entry, so a mismatch shows up
/// as a test failure rather than silent drift.
const METHODS: &[(&str, &str, bool)] = &[
    (
        "fill_path",
        "path: &::mirui::render::path::Path, clip: &::mirui::types::Rect, color: &::mirui::types::Color, opa: u8",
        false,
    ),
    (
        "stroke_path",
        "path: &::mirui::render::path::Path, clip: &::mirui::types::Rect, width: ::mirui::types::Fixed, color: &::mirui::types::Color, opa: u8",
        false,
    ),
    (
        "blit",
        "src: &::mirui::render::texture::Texture, src_rect: &::mirui::types::Rect, dst: ::mirui::types::Point, dst_size: ::mirui::types::Point, clip: &::mirui::types::Rect",
        false,
    ),
    (
        "clear",
        "area: &::mirui::types::Rect, color: &::mirui::types::Color",
        false,
    ),
    (
        "draw_label",
        "pos: &::mirui::types::Point, text: &[u8], font: &::mirui::render::font::Font, clip: &::mirui::types::Rect, color: &::mirui::types::Color, opa: u8",
        false,
    ),
    ("flush", "", false),
    (
        "fill_rect",
        "area: &::mirui::types::Rect, clip: &::mirui::types::Rect, color: &::mirui::types::Color, radius: ::mirui::types::Fixed, opa: u8",
        true,
    ),
    (
        "stroke_rect",
        "area: &::mirui::types::Rect, clip: &::mirui::types::Rect, width: ::mirui::types::Fixed, color: &::mirui::types::Color, radius: ::mirui::types::Fixed, opa: u8",
        true,
    ),
    (
        "draw_line",
        "p1: ::mirui::types::Point, p2: ::mirui::types::Point, clip: &::mirui::types::Rect, width: ::mirui::types::Fixed, color: &::mirui::types::Color, opa: u8",
        true,
    ),
    (
        "draw_arc",
        "center: ::mirui::types::Point, radius: ::mirui::types::Fixed, start_angle: ::mirui::types::Fixed, end_angle: ::mirui::types::Fixed, clip: &::mirui::types::Rect, width: ::mirui::types::Fixed, color: &::mirui::types::Color, opa: u8",
        true,
    ),
];

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<ComposeInput>(input) {
        Ok(parsed) => parsed.emit(),
        Err(e) => e.to_compile_error(),
    }
}

struct ComposeInput {
    vis: Visibility,
    name: Ident,
    fields: Vec<FieldDecl>,
    routes: Vec<Route>,
}

struct FieldDecl {
    name: Ident,
    #[allow(dead_code)]
    ty: syn::Type,
}

struct Route {
    /// `default` or a method name from Canvas.
    method: Ident,
    field: Ident,
}

/// Best-match hint for an unknown method name. Returns `Some(name)` only
/// when the Levenshtein distance is ≤ 2, to avoid suggesting random methods.
fn closest_known_method(query: &str) -> Option<&'static str> {
    crate::diag::closest(query, METHODS.iter().map(|(name, _, _)| *name), 2)
}

fn closest_field(query: &str, fields: &[FieldDecl]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for f in fields {
        let name = f.name.to_string();
        let d = crate::diag::levenshtein(query, &name);
        if d <= 2 && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, name));
        }
    }
    best.map(|(_, n)| n)
}

/// Build one `fn name(&mut self, <params>) { self.<field>.name(<args>) }`.
/// `params_src` is a raw parameter list like `"x: i32, y: i32"` (or empty).
/// Parameter names are extracted via syn::parse so we don't hand-roll parsing.
fn gen_forwarder(method: &str, params_src: &str, field: &Ident) -> TokenStream {
    let method_ident = format_ident!("{method}");
    let params_ts: TokenStream = params_src
        .parse()
        .expect("hard-coded METHODS entry must parse");

    let arg_names: Vec<Ident> = if params_src.is_empty() {
        Vec::new()
    } else {
        let parser = syn::punctuated::Punctuated::<syn::FnArg, Token![,]>::parse_terminated;
        let parsed = parser
            .parse2(params_ts.clone())
            .expect("METHODS entry must parse as FnArg list");
        parsed
            .into_iter()
            .map(|arg| match arg {
                syn::FnArg::Typed(pt) => match *pt.pat {
                    syn::Pat::Ident(pi) => pi.ident,
                    _ => panic!("METHODS params must use simple `name: type` patterns"),
                },
                syn::FnArg::Receiver(_) => unreachable!("METHODS entries have no self"),
            })
            .collect()
    };

    if params_src.is_empty() {
        quote! {
            fn #method_ident(&mut self) {
                self.#field.#method_ident()
            }
        }
    } else {
        quote! {
            #[allow(clippy::too_many_arguments)]
            fn #method_ident(&mut self, #params_ts) {
                self.#field.#method_ident(#(#arg_names),*)
            }
        }
    }
}

impl Parse for ComposeInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let vis: Visibility = input.parse()?;
        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;

        let struct_body;
        braced!(struct_body in input);
        let mut fields = Vec::new();
        while !struct_body.is_empty() {
            let field_name: Ident = struct_body.parse()?;
            struct_body.parse::<Token![:]>()?;
            let ty: syn::Type = struct_body.parse()?;
            fields.push(FieldDecl {
                name: field_name,
                ty,
            });
            if struct_body.peek(Token![,]) {
                struct_body.parse::<Token![,]>()?;
            }
        }

        let route_kw: Ident = input.parse()?;
        if route_kw != "route" {
            return Err(syn::Error::new(route_kw.span(), "expected `route` block"));
        }
        let route_body;
        braced!(route_body in input);
        let mut routes = Vec::new();
        while !route_body.is_empty() {
            let method: Ident = route_body.parse()?;
            route_body.parse::<Token![=>]>()?;
            let field: Ident = route_body.parse()?;
            routes.push(Route { method, field });
            if route_body.peek(Token![,]) {
                route_body.parse::<Token![,]>()?;
            }
        }

        Ok(ComposeInput {
            vis,
            name,
            fields,
            routes,
        })
    }
}

impl ComposeInput {
    fn emit(&self) -> TokenStream {
        if let Err(e) = self.validate() {
            return e.to_compile_error();
        }

        // __B prefix to avoid colliding with user type names.
        let generic_params: Vec<Ident> = (0..self.fields.len())
            .map(|i| format_ident!("__B{i}"))
            .collect();

        let struct_fields = self.fields.iter().zip(&generic_params).map(|(f, g)| {
            let name = &f.name;
            quote! { pub #name: #g }
        });

        let vis = &self.vis;
        let name = &self.name;

        let default_field = self
            .routes
            .iter()
            .find(|r| r.method == "default")
            .map(|r| &r.field)
            .expect("validate() guarantees a default route");

        let method_impls = METHODS
            .iter()
            .filter_map(|(mname, params, is_default_impl)| {
                let explicit = self.routes.iter().find(|r| r.method == *mname);
                let target_field: &Ident = match (explicit, is_default_impl) {
                    (Some(r), _) => &r.field,
                    (None, false) => default_field,
                    // Unrouted default-impl method → skip, trait default handles it.
                    (None, true) => return None,
                };
                Some(gen_forwarder(mname, params, target_field))
            });

        quote! {
            #vis struct #name<#(#generic_params),*> {
                #(#struct_fields,)*
            }

            impl<#(#generic_params),*> ::mirui::render::canvas::Canvas for #name<#(#generic_params),*>
            where
                #(#generic_params: ::mirui::render::canvas::Canvas,)*
            {
                #(#method_impls)*
            }

            impl<#(#generic_params),*> ::mirui::render::renderer::Renderer for #name<#(#generic_params),*>
            where
                #(#generic_params: ::mirui::render::canvas::Canvas,)*
            {
                fn draw(&mut self, cmd: &::mirui::render::DrawCommand, clip: &::mirui::types::Rect) {
                    use ::mirui::render::canvas::Canvas;
                    assert!(
                        cmd.transform().is_identity(),
                        "widget transform not yet supported"
                    );
                    match cmd {
                        ::mirui::render::DrawCommand::Fill { area, color, radius, opa, .. } => {
                            self.fill_rect(area, clip, color, *radius, *opa);
                        }
                        ::mirui::render::DrawCommand::Border { area, color, width, radius, opa, .. } => {
                            self.stroke_rect(area, clip, *width, color, *radius, *opa);
                        }
                        ::mirui::render::DrawCommand::Blit { pos, size, texture, .. } => {
                            let src_rect = ::mirui::types::Rect::new(0, 0, texture.width, texture.height);
                            self.blit(texture, &src_rect, *pos, *size, clip);
                        }
                        ::mirui::render::DrawCommand::Label { pos, text, font, color, opa, .. } => {
                            self.draw_label(pos, text, font, clip, color, *opa);
                        }
                        ::mirui::render::DrawCommand::Line { p1, p2, color, width, opa, .. } => {
                            self.draw_line(*p1, *p2, clip, *width, color, *opa);
                        }
                        ::mirui::render::DrawCommand::Arc {
                            center, radius, start_angle, end_angle, color, width, opa, ..
                        } => {
                            self.draw_arc(*center, *radius, *start_angle, *end_angle, clip, *width, color, *opa);
                        }
                        ::mirui::render::DrawCommand::FillPath { path, color, opa, .. } => {
                            self.fill_path(path, clip, color, *opa);
                        }
                    }
                }

                fn flush(&mut self) {
                    ::mirui::render::canvas::Canvas::flush(self);
                }
            }
        }
    }

    fn validate(&self) -> Result<()> {
        if self.fields.is_empty() {
            return Err(syn::Error::new(
                self.name.span(),
                "compose_backend! struct must have at least one field",
            ));
        }
        for (i, f) in self.fields.iter().enumerate() {
            for g in &self.fields[i + 1..] {
                if f.name == g.name {
                    return Err(syn::Error::new(
                        g.name.span(),
                        format!("field `{}` declared more than once", g.name),
                    ));
                }
            }
        }
        for r in &self.routes {
            if r.method != "default" && !METHODS.iter().any(|(n, _, _)| r.method == *n) {
                let suggestion = closest_known_method(&r.method.to_string());
                let msg = match suggestion {
                    Some(name) => format!(
                        "unknown Canvas method `{}` — did you mean `{name}`?",
                        r.method
                    ),
                    None => format!("unknown Canvas method `{}`", r.method),
                };
                return Err(syn::Error::new(r.method.span(), msg));
            }
            if !self.fields.iter().any(|f| f.name == r.field) {
                let suggestion = closest_field(&r.field.to_string(), &self.fields);
                let msg = match suggestion {
                    Some(name) => format!(
                        "field `{}` not declared in struct body — did you mean `{name}`?",
                        r.field
                    ),
                    None => format!("field `{}` not declared in struct body", r.field),
                };
                return Err(syn::Error::new(r.field.span(), msg));
            }
        }
        for (i, r) in self.routes.iter().enumerate() {
            for s in &self.routes[i + 1..] {
                if r.method == s.method {
                    return Err(syn::Error::new(
                        s.method.span(),
                        format!(
                            "method `{}` routed more than once (also at earlier position)",
                            s.method
                        ),
                    ));
                }
            }
        }
        if !self.routes.iter().any(|r| r.method == "default") {
            return Err(syn::Error::new(
                self.name.span(),
                "compose_backend! requires a `default => <field>` route",
            ));
        }
        Ok(())
    }
}
