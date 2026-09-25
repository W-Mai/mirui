use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, ItemFn, Path, Token, Type, bracketed, parenthesized};

struct Watch {
    field: Ident,
    getter: Ident,
}

struct ViewArgs {
    component: Type,
    read: Vec<Ident>,
    watch: Vec<Watch>,
    priority: Expr,
    attach: Option<Path>,
    gesture: Option<Path>,
    systems: Vec<Expr>,
}

impl Parse for ViewArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut component = None;
        let mut read = Vec::new();
        let mut watch = Vec::new();
        let mut priority = None;
        let mut attach = None;
        let mut gesture = None;
        let mut systems = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "component" => {
                    input.parse::<Token![=]>()?;
                    if component.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate component"));
                    }
                }
                "priority" => {
                    input.parse::<Token![=]>()?;
                    if priority.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate priority"));
                    }
                }
                "attach" => {
                    input.parse::<Token![=]>()?;
                    if attach.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate attach"));
                    }
                }
                "gesture" => {
                    input.parse::<Token![=]>()?;
                    if gesture.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate gesture"));
                    }
                }
                "read" => {
                    let content;
                    parenthesized!(content in input);
                    let names = content.parse_terminated(Ident::parse, Token![,])?;
                    for name in names {
                        if read.contains(&name) {
                            return Err(syn::Error::new(name.span(), "duplicate read field"));
                        }
                        read.push(name);
                    }
                }
                "watch" => {
                    let content;
                    parenthesized!(content in input);
                    let expressions: Punctuated<Expr, Token![,]> =
                        content.parse_terminated(Expr::parse, Token![,])?;
                    for expression in expressions {
                        let Expr::MethodCall(call) = expression else {
                            return Err(syn::Error::new_spanned(
                                expression,
                                "watch entries must be `field.observed_getter()`",
                            ));
                        };
                        if !call.args.is_empty() || call.turbofish.is_some() {
                            return Err(syn::Error::new_spanned(
                                call,
                                "watch getters cannot take arguments",
                            ));
                        }
                        let Expr::Path(receiver) = *call.receiver else {
                            return Err(syn::Error::new_spanned(
                                call.receiver,
                                "watch receiver must be a direct component field",
                            ));
                        };
                        let Some(field) = receiver.path.get_ident().cloned() else {
                            return Err(syn::Error::new_spanned(
                                receiver,
                                "watch receiver must be a direct component field",
                            ));
                        };
                        if watch.iter().any(|entry: &Watch| {
                            entry.field == field && entry.getter == call.method
                        }) {
                            return Err(syn::Error::new(
                                call.method.span(),
                                "duplicate watch getter",
                            ));
                        }
                        watch.push(Watch {
                            field,
                            getter: call.method,
                        });
                    }
                }
                "systems" => {
                    input.parse::<Token![=]>()?;
                    let content;
                    bracketed!(content in input);
                    systems.extend(content.parse_terminated(Expr::parse, Token![,])?);
                }
                _ => return Err(syn::Error::new(key.span(), "unknown #[view] argument")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            component: component
                .ok_or_else(|| syn::Error::new(input.span(), "view requires `component = Type`"))?,
            read,
            watch,
            priority: priority.unwrap_or_else(|| syn::parse_quote!(60u8)),
            attach,
            gesture,
            systems,
        })
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: ViewArgs = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;
    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(&func.sig, "views cannot be async"));
    }
    if !matches!(func.sig.output, syn::ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &func.sig.output,
            "views return `()`",
        ));
    }
    let name = &func.sig.ident;
    let visibility = &func.vis;
    let component = &args.component;
    let priority = &args.priority;
    let mut call_args = Vec::new();
    let mut uses_theme = false;
    let mut uses_component = false;
    for input in &func.sig.inputs {
        let syn::FnArg::Typed(param) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "views cannot have a receiver",
            ));
        };
        let syn::Pat::Ident(pattern) = &*param.pat else {
            return Err(syn::Error::new_spanned(
                &param.pat,
                "view parameters must be named",
            ));
        };
        let ident = &pattern.ident;
        let name_text = ident.to_string();
        if args.read.contains(ident) {
            let syn::Type::Reference(reference) = &*param.ty else {
                return Err(syn::Error::new_spanned(
                    &param.ty,
                    "read parameters use `&Model`",
                ));
            };
            if reference.mutability.is_some() {
                return Err(syn::Error::new_spanned(
                    &param.ty,
                    "views cannot mutably borrow a model",
                ));
            }
        } else if name_text == "theme" {
            uses_theme = true;
        } else if name_text == "component" {
            uses_component = true;
        } else if !matches!(name_text.as_str(), "renderer" | "rect" | "ctx") {
            return Err(syn::Error::new(ident.span(), "unknown view parameter"));
        }
        call_args.push(ident.clone());
    }
    for field in &args.read {
        if !call_args.contains(field) {
            return Err(syn::Error::new(
                field.span(),
                "read field needs a matching view parameter",
            ));
        }
    }
    let mut render_call = quote!(super::#name(#(#call_args),*));
    for field in args.read.iter().rev() {
        render_call = quote! {
            ::mirui::core::model::ModelHandle::read(&__component.#field, |#field| {
                #render_call
            })
        };
    }
    let theme_lookup = uses_theme.then(|| {
        quote! {
            let theme = world.resource::<::mirui::ui::Theme>()
                .expect("App must install Theme before rendering");
        }
    });
    let component_alias = uses_component.then(|| quote!(let component = __component;));
    let generics = &func.sig.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let render_fn = if generics.params.is_empty() {
        quote!(render)
    } else {
        quote!(render::#type_generics)
    };
    let observe_fn = if generics.params.is_empty() {
        quote!(observe)
    } else {
        quote!(observe::#type_generics)
    };
    let observation = if args.watch.is_empty() {
        quote!()
    } else {
        let subscriptions = args.watch.iter().map(|watch| {
            let field = &watch.field;
            let subscribe = format_ident!("__mirui_subscribe_{}", watch.getter);
            quote!(bindings.watch(__component.#field.#subscribe(world, entity));)
        });
        quote! {
            fn observe #impl_generics (
                world: &::mirui::ecs::World,
                entity: ::mirui::ecs::Entity,
                bindings: &mut ::mirui::ui::view::ViewObservationBindings,
            ) #where_clause {
                let __component = world.get::<#component>(entity).expect("view component");
                #(#subscriptions)*
            }
        }
    };
    let with_observation = (!args.watch.is_empty()).then(|| quote!(.with_observation(#observe_fn)));
    let with_attach = args.attach.as_ref().map(|path| quote!(.with_attach(#path)));
    let with_gesture = args
        .gesture
        .as_ref()
        .map(|path| quote!(.with_internal_gesture(#path)));
    let system_array = (!args.systems.is_empty()).then(|| {
        let systems = &args.systems;
        quote!(const SYSTEMS: &[::mirui::ecs::System] = &[#(#systems),*];)
    });
    let with_systems = (!args.systems.is_empty()).then(|| quote!(.with_systems(SYSTEMS)));
    Ok(quote! {
        #func

        #[allow(non_snake_case)]
        #visibility mod #name {
            #[allow(unused_imports)]
            use super::*;

            #system_array

            #[allow(unused_variables)]
            fn render #impl_generics (
                renderer: &mut dyn ::mirui::render::renderer::Renderer,
                world: &::mirui::ecs::World,
                entity: ::mirui::ecs::Entity,
                rect: &::mirui::types::Rect,
                ctx: &mut ::mirui::ui::view::ViewCtx,
            ) #where_clause {
                ::mirui::core::model::with_model_read_only(|| {
                    let __component = world.get::<#component>(entity).expect("view component");
                    #theme_lookup
                    #component_alias
                    #render_call;
                });
            }

            #observation

            pub fn view #impl_generics () -> ::mirui::ui::view::View #where_clause {
                ::mirui::ui::view::View::new(stringify!(#name), #priority, #render_fn)
                    .with_filter::<#component>()
                    #with_observation
                    #with_attach
                    #with_gesture
                    #with_systems
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    #[test]
    fn generated_view_reads_only_declared_fields_and_subscribes_named_getters() {
        let expanded = expand(
            quote!(component = Board, read(game), watch(game.visual_revision())),
            quote!(
                fn paint(game: &Game, ctx: &mut ViewCtx) {
                    let _ = (game, ctx);
                }
            ),
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("ModelHandle :: read"));
        assert!(expanded.contains("__mirui_subscribe_visual_revision"));
        assert!(expanded.contains("with_model_read_only"));
    }

    #[test]
    fn rejects_indirect_or_parameterized_watch_getters() {
        for watch in [quote!(game.inner.value()), quote!(game.value(1))] {
            let error = expand(
                quote!(component = Board, watch(#watch)),
                quote!(
                    fn paint(ctx: &mut ViewCtx) {
                        let _ = ctx;
                    }
                ),
            )
            .err()
            .unwrap();
            assert!(error.to_string().contains("watch"));
        }
    }

    #[test]
    fn rejects_mutable_model_borrows_and_unknown_parameters() {
        let mutable = expand(
            quote!(component = Board, read(game)),
            quote!(
                fn paint(game: &mut Game) {}
            ),
        )
        .err()
        .unwrap();
        assert!(mutable.to_string().contains("mutably borrow"));

        let unknown = expand(
            quote!(component = Board),
            quote!(
                fn paint(world: &World) {}
            ),
        )
        .err()
        .unwrap();
        assert!(unknown.to_string().contains("unknown view parameter"));
    }
}
