use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::{Ident, Result, Token, Visibility, braced};

/// Canvas method table. Each entry is `(name, parameters, has_default)`.
/// Required methods forward to the shared target. Canvas helpers with default
/// implementations reach that same target through the required primitives.
const METHODS: &[(&str, &str, bool)] = &[
    (
        "fill_path",
        "path: &::mirui::render::path::Path, clip: &::mirui::types::Rect, paint: &::mirui::render::Paint, opa: u8, fill_rule: ::mirui::render::raster::FillRule",
        false,
    ),
    (
        "stroke_path",
        "path: &::mirui::render::path::Path, clip: &::mirui::types::Rect, width: ::mirui::types::Fixed, paint: &::mirui::render::Paint, opa: u8, cap: ::mirui::render::raster::LineCap, join: ::mirui::render::raster::LineJoin, miter_limit: ::mirui::types::Fixed, dash: &[::mirui::types::Fixed]",
        false,
    ),
    (
        "blit",
        "src: &::mirui::render::texture::Texture, src_rect: &::mirui::types::Rect, dst: ::mirui::types::Point, dst_size: ::mirui::types::Point, clip: &::mirui::types::Rect, opa: u8, radius: ::mirui::types::Fixed, composite: ::mirui::render::command::CompositeMode",
        false,
    ),
    (
        "clear",
        "area: &::mirui::types::PhysicalRect, color: &::mirui::types::Color",
        false,
    ),
    (
        "draw_glyph_run",
        "pos: &::mirui::types::Point, glyphs: &[::mirui::text::PositionedGlyph], font: &::mirui::render::font::Font, clip: &::mirui::types::Rect, color: &::mirui::types::Color, opa: u8",
        false,
    ),
    (
        "draw_posed_glyph_run",
        "pos: &::mirui::types::Point, glyphs: ::mirui::render::PosedGlyphs<'_>, font: &::mirui::render::font::Font, clip: &::mirui::types::Rect, color: &::mirui::types::Color, opa: u8",
        false,
    ),
    ("flush", "", false),
    (
        "push_clip",
        "path: &::mirui::render::path::Path, transform: &::mirui::types::Transform, fill_rule: ::mirui::render::raster::FillRule",
        true,
    ),
    ("pop_clip", "", true),
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

const ROUTES: &[&str] = &[
    "fill_path",
    "stroke_path",
    "blit",
    "draw_glyph_run",
    "draw_posed_glyph_run",
    "fill_rect",
    "stroke_rect",
    "draw_line",
    "draw_arc",
    "blur",
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
    /// `default` or a command class handled by a render engine.
    method: Ident,
    field: Ident,
}

/// Best-match hint for an unknown method name. Returns `Some(name)` only
/// when the Levenshtein distance is ≤ 2, to avoid suggesting random methods.
fn closest_known_method(query: &str) -> Option<&'static str> {
    crate::diag::closest(query, ROUTES.iter().copied(), 2)
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

fn gen_forwarder(method: &str, params_src: &str, target: &Ident) -> TokenStream {
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
        let output =
            (method == "flush").then(|| quote! { -> Result<(), ::mirui::render::RenderError> });
        quote! {
            fn #method_ident(&mut self) #output {
                ::mirui::render::canvas::Canvas::#method_ident(&mut self.#target)
            }
        }
    } else {
        quote! {
            #[allow(clippy::too_many_arguments)]
            fn #method_ident(&mut self, #params_ts) {
                ::mirui::render::canvas::Canvas::#method_ident(
                    &mut self.#target,
                    #(#arg_names),*
                )
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
        let target_index = self
            .fields
            .iter()
            .position(|field| field.name == *default_field)
            .expect("validate() guarantees the default field exists");
        let target_generic = &generic_params[target_index];
        let engine_fields: Vec<(&Ident, &Ident)> = self
            .fields
            .iter()
            .zip(&generic_params)
            .filter(|(field, _)| field.name != *default_field)
            .map(|(field, generic)| (&field.name, generic))
            .collect();
        let engine_generics: Vec<&Ident> =
            engine_fields.iter().map(|(_, generic)| *generic).collect();
        let method_impls = METHODS
            .iter()
            .map(|(method, params, _)| gen_forwarder(method, params, default_field));
        let constructor_params = self
            .fields
            .iter()
            .zip(&generic_params)
            .map(|(field, generic)| {
                let field = &field.name;
                quote! { #field: #generic }
            });
        let constructor_fields = self.fields.iter().map(|field| &field.name);

        let routed_field = |method: &str| {
            self.routes
                .iter()
                .find(|route| route.method == method)
                .map_or(default_field, |route| &route.field)
        };
        let route_call = |field: &Ident| {
            if field == default_field {
                quote! {
                    ::mirui::render::renderer::Renderer::route(&self.#default_field, request)
                }
            } else {
                quote! {
                    ::mirui::render::engine::RenderEngine::route(
                        &self.#field,
                        &self.#default_field,
                        request,
                    )
                }
            }
        };
        let submit_call = |field: &Ident| {
            if field == default_field {
                quote! {
                    ::mirui::render::renderer::Renderer::submit(
                        &mut self.#default_field,
                        draw,
                    )
                }
            } else {
                quote! {{
                    ::mirui::render::engine::RenderEngine::begin(
                        &mut self.#field,
                        &mut self.#default_field,
                    )?;
                    let result = ::mirui::render::engine::RenderEngine::submit(
                        &mut self.#field,
                        &mut self.#default_field,
                        draw,
                    );
                    ::mirui::render::engine::RenderEngine::end(
                        &mut self.#field,
                        &mut self.#default_field,
                    );
                    result
                }}
            }
        };
        let prepare_call = |field: &Ident| {
            if field == default_field {
                quote! { ::mirui::render::Renderer::prepare(&mut self.#default_field, request, route, cx) }
            } else {
                quote! { ::mirui::render::RenderEngine::prepare(&mut self.#field, &mut self.#default_field, request, route, cx) }
            }
        };
        let text_submit_call = |field: &Ident| {
            if field == default_field {
                quote! { ::mirui::render::Renderer::submit_text_run(&mut self.#default_field, draw, identity) }
            } else {
                quote! {{
                    ::mirui::render::RenderEngine::begin(&mut self.#field, &mut self.#default_field)?;
                    let result = ::mirui::render::RenderEngine::submit_text_run(&mut self.#field, &mut self.#default_field, draw, identity);
                    ::mirui::render::RenderEngine::end(&mut self.#field, &mut self.#default_field);
                    result
                }}
            }
        };
        let commands = [
            ("Fill", "fill_rect"),
            ("Border", "stroke_rect"),
            ("Blit", "blit"),
            ("GlyphRun", "draw_glyph_run"),
            ("PosedGlyphRun", "draw_posed_glyph_run"),
            ("Line", "draw_line"),
            ("Arc", "draw_arc"),
            ("FillPath", "fill_path"),
            ("StrokePath", "stroke_path"),
            ("PushClip", "default"),
            ("PopClip", "default"),
            ("ApplyBlur", "blur"),
        ];
        let mut route_arms = Vec::new();
        let mut prepare_arms = Vec::new();
        let mut submit_arms = Vec::new();
        let mut text_submit_arms = Vec::new();
        for (variant, method) in commands {
            let ident = format_ident!("{variant}");
            let pattern = if variant == "PopClip" {
                quote! { ::mirui::render::DrawCommand::#ident }
            } else {
                quote! { ::mirui::render::DrawCommand::#ident { .. } }
            };
            let field = routed_field(method);
            let route = route_call(field);
            let prepare = prepare_call(field);
            let submit = submit_call(field);
            route_arms.push(quote! { #pattern => #route });
            prepare_arms.push(quote! { #pattern => #prepare });
            submit_arms.push(quote! { #pattern => #submit });
            if matches!(variant, "GlyphRun" | "PosedGlyphRun") {
                let text = text_submit_call(field);
                text_submit_arms.push(quote! { #pattern => #text });
            }
        }

        quote! {
            #vis struct #name<#(#generic_params),*> {
                #(#struct_fields,)*
            }

            impl<#(#generic_params),*> #name<#(#generic_params),*> {
                pub fn new(#(#constructor_params),*) -> Self {
                    Self { #(#constructor_fields,)* }
                }
            }

            impl<#(#generic_params),*> ::mirui::render::canvas::Canvas for #name<#(#generic_params),*>
            where
                #target_generic: ::mirui::render::canvas::Canvas
                    + ::mirui::render::renderer::Renderer,
                #(#engine_generics: ::mirui::render::engine::RenderEngine<#target_generic>,)*
            {
                #(#method_impls)*
            }

            impl<#(#generic_params),*> ::mirui::render::renderer::Renderer for #name<#(#generic_params),*>
            where
                #target_generic: ::mirui::render::canvas::Canvas
                    + ::mirui::render::renderer::Renderer,
                #(#engine_generics: ::mirui::render::engine::RenderEngine<#target_generic>,)*
            {
                type TargetState = <#target_generic as ::mirui::render::Renderer>::TargetState;

                fn plan_target(
                    &self,
                    layout: ::mirui::render::PlaneLayout,
                ) -> Result<Option<::mirui::render::TargetPlan>, ::mirui::render::RenderError> {
                    ::mirui::render::Renderer::plan_target(&self.#default_field, layout)
                }

                fn can_reuse_target(
                    &self,
                    target: &dyn ::mirui::render::BackendTarget,
                    plan: &::mirui::render::TargetPlan,
                ) -> Result<bool, ::mirui::render::RenderError> {
                    ::mirui::render::Renderer::can_reuse_target(&self.#default_field, target, plan)
                }

                fn create_target(
                    &mut self,
                    plan: &::mirui::render::TargetPlan,
                ) -> Result<::mirui::render::target::__private::Box<dyn ::mirui::render::BackendTarget>, ::mirui::render::RenderError> {
                    ::mirui::render::Renderer::create_target(&mut self.#default_field, plan)
                }

                fn bind_target(
                    &mut self,
                    target: &mut dyn ::mirui::render::BackendTarget,
                    viewport: ::mirui::types::Viewport,
                    update: ::mirui::render::TargetUpdate,
                    workspace: &mut [u8],
                ) -> Result<Self::TargetState, ::mirui::render::RenderError> {
                    ::mirui::render::Renderer::bind_target(
                        &mut self.#default_field, target, viewport, update, workspace,
                    )
                }

                fn restore_target(
                    &mut self,
                    state: Self::TargetState,
                    completed: bool,
                ) -> Result<(), ::mirui::render::RenderError> {
                    ::mirui::render::Renderer::restore_target(
                        &mut self.#default_field, state, completed,
                    )
                }

                fn route(
                    &self,
                    request: &::mirui::render::renderer::DrawRequest<'_, '_>,
                ) -> Result<::mirui::render::renderer::RenderRoute, ::mirui::render::renderer::RenderError> {
                    match request.command {
                        #(#route_arms,)*
                    }
                }

                fn submit(
                    &mut self,
                    draw: &::mirui::render::PreparedDraw<'_>,
                ) -> Result<(), ::mirui::render::renderer::RenderError> {
                    match draw.request().command {
                        #(#submit_arms,)*
                    }
                }

                fn submit_text_run(
                    &mut self,
                    draw: &::mirui::render::PreparedDraw<'_>,
                    identity: ::mirui::render::TextRunIdentity,
                ) -> Result<(), ::mirui::render::RenderError> {
                    match draw.request().command {
                        #(#text_submit_arms,)*
                        _ => ::mirui::render::Renderer::submit(self, draw),
                    }
                }

                fn prepare<'data>(
                    &mut self,
                    request: ::mirui::render::DrawRequest<'_, 'data>,
                    route: ::mirui::render::RenderRoute,
                    cx: &mut ::core::task::Context<'_>,
                ) -> ::core::task::Poll<Result<::mirui::render::PreparedDraw<'data>, ::mirui::render::RenderError>> {
                    match request.command {
                        #(#prepare_arms,)*
                    }
                }

                fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                    ::mirui::render::renderer::Renderer::flush(&mut self.#default_field)
                }

                fn output_scale(&self) -> ::mirui::types::Fixed {
                    ::mirui::render::renderer::Renderer::output_scale(&self.#default_field)
                }

                fn plan_scope(
                    &self,
                    bounds: &::mirui::types::Rect,
                ) -> Result<
                    ::mirui::render::renderer::FallbackRegion,
                    ::mirui::render::renderer::RenderError,
                > {
                    ::mirui::render::renderer::Renderer::plan_scope(
                        &self.#default_field,
                        bounds,
                    )
                }

                fn supports_offscreen(&self) -> bool {
                    ::mirui::render::renderer::Renderer::supports_offscreen(&self.#default_field)
                }

                fn offscreen_format(&self) -> Option<::mirui::render::texture::ColorFormat> {
                    ::mirui::render::renderer::Renderer::offscreen_format(&self.#default_field)
                }

                fn read_target_region(
                    &self,
                    src: &::mirui::types::Rect,
                    dst: &mut ::mirui::render::texture::Texture,
                ) -> Result<(), ::mirui::render::renderer::RenderError> {
                    ::mirui::render::renderer::Renderer::read_target_region(
                        &self.#default_field,
                        src,
                        dst,
                    )
                }

                fn sample_target_region(
                    &self,
                    src: &::mirui::types::Rect,
                ) -> Result<
                    Option<::mirui::render::texture::Texture<'static>>,
                    ::mirui::render::renderer::RenderError,
                > {
                    ::mirui::render::renderer::Renderer::sample_target_region(
                        &self.#default_field,
                        src,
                    )
                }

                fn modify_target_region(
                    &mut self,
                    src: &::mirui::types::Rect,
                    f: &mut dyn FnMut(
                        &mut ::mirui::render::texture::Texture,
                    ) -> Result<(), ::mirui::render::renderer::RenderError>,
                ) -> Result<bool, ::mirui::render::renderer::RenderError> {
                    ::mirui::render::renderer::Renderer::modify_target_region(
                        &mut self.#default_field,
                        src,
                        f,
                    )
                }

                fn prepare_readback(
                    &mut self,
                    src: &::mirui::types::Rect,
                ) -> Result<(), ::mirui::render::renderer::RenderError> {
                    ::mirui::render::renderer::Renderer::prepare_readback(
                        &mut self.#default_field,
                        src,
                    )
                }

                fn supports_scroll_blit(&self) -> bool {
                    ::mirui::render::renderer::Renderer::supports_scroll_blit(&self.#default_field)
                }

                fn scroll_target_region(
                    &mut self,
                    area: &::mirui::types::Rect,
                    dx: ::mirui::types::Fixed,
                    dy: ::mirui::types::Fixed,
                ) -> Result<(), ::mirui::render::renderer::RenderError> {
                    ::mirui::render::renderer::Renderer::scroll_target_region(
                        &mut self.#default_field,
                        area,
                        dx,
                        dy,
                    )
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
            if r.method != "default" && !ROUTES.iter().any(|name| r.method == *name) {
                let suggestion = closest_known_method(&r.method.to_string());
                let msg = match suggestion {
                    Some(name) => format!(
                        "unknown render route `{}` — did you mean `{name}`?",
                        r.method
                    ),
                    None => format!("unknown render route `{}`", r.method),
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
