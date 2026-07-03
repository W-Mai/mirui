use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;

use xrune::ds_node::{DsTree, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

use crate::MiruiRune;

pub struct MoldParam {
    name: syn::Ident,
    ty: syn::Type,
}

impl Parse for MoldParam {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        input.parse::<syn::Token![:]>()?;
        let ty: syn::Type = input.parse()?;
        Ok(MoldParam { name, ty })
    }
}

pub struct MoldInput {
    name: syn::Ident,
    params: Vec<MoldParam>,
    children: Vec<DsTreeRef>,
}

impl Parse for MoldInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;

        let params = if input.peek(syn::token::Paren) {
            let param_buf;
            syn::parenthesized!(param_buf in input);
            let punctuated: Punctuated<MoldParam, syn::Token![,]> =
                Punctuated::parse_terminated(&param_buf)?;
            punctuated.into_iter().collect()
        } else {
            Vec::new()
        };

        let body_buf;
        syn::braced!(body_buf in input);
        let mut children = Vec::new();
        while !body_buf.is_empty() {
            let child = DsTree::parse(&body_buf)?.into_ref();
            child.borrow_mut().set_parent(child.clone());
            children.push(child);
        }
        if children.is_empty() {
            return Err(syn::Error::new_spanned(
                &name,
                "mold! body must declare at least one node (widget tree or `@@slot`)",
            ));
        }
        Ok(MoldInput {
            name,
            params,
            children,
        })
    }
}

pub fn expand(input: TokenStream) -> TokenStream {
    let MoldInput {
        name,
        params,
        children,
    } = match syn::parse2(input) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error(),
    };

    let attach_fn = format_ident!("{}_attach", to_snake_case(&name.to_string()));
    let view_fn = format_ident!("{}_view", to_snake_case(&name.to_string()));
    let render_fn = format_ident!("__{}_render", to_snake_case(&name.to_string()));

    let world_expr: TokenStream = quote! { world };
    let entity_expr: TokenStream = quote! { entity };
    let mut rune = MiruiRune::new_mold(world_expr.clone(), entity_expr.clone());
    for child in &children {
        decipher(child, &mut rune);
    }
    let body_tokens = rune.seal();

    let (struct_decl, param_binds) = if params.is_empty() {
        (
            quote! {
                #[derive(Default)]
                pub struct #name;
            },
            quote! {},
        )
    } else {
        let field_defs = params.iter().map(|p| {
            let n = &p.name;
            let t = &p.ty;
            quote! { pub #n: #t }
        });
        let bind_names: Vec<_> = params.iter().map(|p| &p.name).collect();
        (
            quote! {
                #[derive(Default)]
                pub struct #name {
                    #( #field_defs, )*
                }
            },
            quote! {
                let __mold_params = match world.get::<#name>(entity) {
                    Some(p) => (#( ::core::clone::Clone::clone(&p.#bind_names), )*),
                    None => return,
                };
                let ( #( #bind_names, )* ) = __mold_params;
            },
        )
    };

    let existence_check = if params.is_empty() {
        quote! {
            if world.get::<#name>(entity).is_none() {
                return;
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #struct_decl

        impl ::mirui::ecs::Component for #name {}

        fn #attach_fn(world: &mut ::mirui::ecs::World, entity: ::mirui::ecs::Entity) {
            #existence_check
            if world.get::<::mirui::ui::NicheMap>(entity).is_some() {
                return;
            }
            #param_binds

            let mut __mold_niche_map = ::mirui::ui::NicheMap::new();
            let _ = #body_tokens;
            world.insert(entity, __mold_niche_map);
        }

        fn #render_fn(
            _: &mut dyn ::mirui::render::renderer::Renderer,
            _: &::mirui::ecs::World,
            _: ::mirui::ecs::Entity,
            _: &::mirui::types::Rect,
            _: &mut ::mirui::ui::view::ViewCtx,
        ) {}

        pub fn #view_fn() -> ::mirui::ui::View {
            ::mirui::ui::View::new(stringify!(#name), 60, #render_fn)
                .with_filter::<#name>()
                .with_attach(#attach_fn)
        }
    }
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    out
}
