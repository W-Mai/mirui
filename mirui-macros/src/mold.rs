use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;

use xrune::ds_node::node_enum::DsNode;
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

pub enum MoldInput {
    Decl {
        name: syn::Ident,
        params: Vec<MoldParam>,
        children: Vec<DsTreeRef>,
    },
    Expr(syn::Ident),
}

impl Parse for MoldInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;

        if input.is_empty() {
            return Ok(MoldInput::Expr(name));
        }

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
        Ok(MoldInput::Decl {
            name,
            params,
            children,
        })
    }
}

pub fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<MoldInput>(input) {
        Ok(MoldInput::Expr(name)) => quote! { <#name>::__view() },
        Ok(MoldInput::Decl {
            name,
            params,
            children,
        }) => expand_decl(name, params, &children),
        Err(e) => e.to_compile_error(),
    }
}

fn expand_decl(name: syn::Ident, params: Vec<MoldParam>, children: &[DsTreeRef]) -> TokenStream {
    let world_expr: TokenStream = quote! { world };
    let entity_expr: TokenStream = quote! { entity };
    let mut rune = MiruiRune::new_mold(world_expr.clone(), entity_expr.clone());
    for child in children {
        decipher(child, &mut rune);
    }
    let body_tokens = rune.seal();

    let mut slot_names = Vec::new();
    for child in children {
        collect_decl_slots(child, &mut slot_names);
    }

    let slot_methods: Vec<TokenStream> = slot_names
        .iter()
        .map(|slot| {
            let m = format_ident!("__slot_{}", slot);
            quote! {
                #[doc(hidden)]
                #[allow(non_snake_case)]
                pub fn #m() {}
            }
        })
        .collect();

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

        impl #name {
            #( #slot_methods )*

            #[doc(hidden)]
            pub fn __attach(world: &mut ::mirui::ecs::World, entity: ::mirui::ecs::Entity) {
                #existence_check
                if world.get::<::mirui::ui::NicheMap>(entity).is_some() {
                    return;
                }
                #param_binds

                let mut __mold_niche_map = ::mirui::ui::NicheMap::new();
                let _ = #body_tokens;
                world.insert(entity, __mold_niche_map);
            }

            #[doc(hidden)]
            pub fn __render(
                _: &mut dyn ::mirui::render::renderer::Renderer,
                _: &::mirui::ecs::World,
                _: ::mirui::ecs::Entity,
                _: &::mirui::types::Rect,
                _: &mut ::mirui::ui::view::ViewCtx,
            ) {}

            pub fn __view() -> ::mirui::ui::View {
                ::mirui::ui::View::new(stringify!(#name), 60, Self::__render)
                    .with_filter::<Self>()
                    .with_attach(Self::__attach)
            }
        }
    }
}

fn collect_decl_slots(tree: &DsTreeRef, out: &mut Vec<String>) {
    let borrowed = tree.borrow();
    if let DsNode::Niche(n) = borrowed.get_node()
        && n.is_declaration()
    {
        let name = n.get_name().to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    for child in borrowed.get_children() {
        collect_decl_slots(child, out);
    }
}
