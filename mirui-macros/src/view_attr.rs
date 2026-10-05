use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Expr, FnArg, Ident, ItemFn, LitStr, Pat, Path, ReturnType, Token, Type, bracketed};

use crate::binding::{
    BindingAccess, BindingCapability, BindingCardinality, BindingInvalidation, BindingSource,
    BindingSpec, BindingSyntax, ComponentSource, ComponentTarget, LogicalType, ModelSource,
    RoleKind, VIEW_ROLES, parse_resource_role,
};
use crate::child_scope::ChildScope;
use crate::function_attrs::{FunctionAttributes, classify_function_attributes};

struct ViewArgs {
    component: Type,
    name: Option<LitStr>,
    priority: Expr,
    invalidation: BindingInvalidation,
    attach: Option<Path>,
    gesture: Option<Path>,
    systems: Vec<Expr>,
}

impl Parse for ViewArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut component = None;
        let mut name = None;
        let mut priority = None;
        let mut invalidation = None;
        let mut attach = None;
        let mut gesture = None;
        let mut systems = None;

        while !input.is_empty() {
            let key = input.parse::<Ident>()?;
            match key.to_string().as_str() {
                "component" => {
                    input.parse::<Token![=]>()?;
                    if component.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `component`"));
                    }
                }
                "name" => {
                    input.parse::<Token![=]>()?;
                    if name.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `name`"));
                    }
                }
                "priority" => {
                    input.parse::<Token![=]>()?;
                    if priority.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `priority`"));
                    }
                }
                "invalidation" => {
                    input.parse::<Token![=]>()?;
                    let policy = input.parse::<Ident>().map_err(|_| {
                        syn::Error::new(input.span(), "`invalidation` expects `Visual` or `Layout`")
                    })?;
                    if input.peek(Token![::]) {
                        return Err(syn::Error::new(
                            input.span(),
                            "`invalidation` expects the bare name `Visual` or `Layout`",
                        ));
                    }
                    let policy = match policy.to_string().as_str() {
                        "Visual" => BindingInvalidation::Visual,
                        "Layout" => BindingInvalidation::Layout,
                        _ => {
                            return Err(syn::Error::new(
                                policy.span(),
                                "unknown View invalidation; expected `Visual` or `Layout`",
                            ));
                        }
                    };
                    if invalidation.replace(policy).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `invalidation`"));
                    }
                }
                "attach" => {
                    input.parse::<Token![=]>()?;
                    if attach.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `attach`"));
                    }
                }
                "gesture" => {
                    input.parse::<Token![=]>()?;
                    if gesture.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `gesture`"));
                    }
                }
                "systems" => {
                    input.parse::<Token![=]>()?;
                    if systems.is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `systems`"));
                    }
                    let content;
                    bracketed!(content in input);
                    systems = Some(
                        content
                            .parse_terminated(Expr::parse, Token![,])?
                            .into_iter()
                            .collect(),
                    );
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown `#[view]` argument; expected `component`, `name`, `priority`, `invalidation`, `attach`, `gesture`, or `systems`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            component: component.ok_or_else(|| {
                syn::Error::new(input.span(), "`#[view]` requires `component = T`")
            })?,
            name,
            priority: priority.unwrap_or_else(|| syn::parse_quote!(60u8)),
            invalidation: invalidation.unwrap_or(BindingInvalidation::Visual),
            attach,
            gesture,
            systems: systems.unwrap_or_default(),
        })
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<ViewArgs>(attr)?;
    let function = syn::parse2::<ItemFn>(item)?;
    validate_function(&function)?;
    let bindings = parse_parameters(&function.sig.inputs, &args.component, args.invalidation)?;
    lower(function, args, bindings)
}

fn validate_function(function: &ItemFn) -> syn::Result<()> {
    let signature = &function.sig;
    if let Some(token) = signature.constness {
        return Err(syn::Error::new(token.span(), "views cannot be `const`"));
    }
    if let Some(token) = signature.asyncness {
        return Err(syn::Error::new(token.span(), "views cannot be async"));
    }
    if let Some(token) = signature.unsafety {
        return Err(syn::Error::new(token.span(), "views cannot be unsafe"));
    }
    if let Some(abi) = &signature.abi {
        return Err(syn::Error::new_spanned(abi, "views cannot declare an ABI"));
    }
    if let Some(variadic) = &signature.variadic {
        return Err(syn::Error::new_spanned(
            variadic,
            "views cannot be variadic",
        ));
    }
    if !matches!(signature.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &signature.output,
            "views return `()` implicitly",
        ));
    }
    Ok(())
}

fn parse_parameters(
    inputs: &Punctuated<FnArg, Token![,]>,
    component_type: &Type,
    invalidation: BindingInvalidation,
) -> syn::Result<Vec<BindingSpec>> {
    let mut bindings = Vec::with_capacity(inputs.len());
    let mut names = HashSet::with_capacity(inputs.len());
    let mut watches = HashSet::new();
    let mut component_binding = None;
    let mut capabilities = Vec::new();

    for input in inputs {
        let binding = parse_parameter(input, component_type, invalidation)?;
        if !names.insert(binding.name.to_string()) {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("view binding `{}` is declared more than once", binding.name),
            ));
        }
        match &binding.source {
            BindingSource::Component(_) => {
                if component_binding.replace(binding.name.clone()).is_some() {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        "`#[view]` accepts at most one `component!()` parameter",
                    ));
                }
            }
            BindingSource::Capability(capability) => {
                if capabilities.contains(capability) {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        format!(
                            "`#[view]` accepts at most one `{}!()` parameter",
                            capability_name(*capability),
                        ),
                    ));
                }
                capabilities.push(*capability);
            }
            _ => {}
        }
        if let BindingSource::Model(ModelSource::ViewComponentField { field }) = &binding.source {
            let field = field.to_token_stream().to_string();
            for selector in &binding.watch_selectors {
                let key = (field.clone(), selector.to_string());
                if !watches.insert(key) {
                    return Err(syn::Error::new(
                        selector.span(),
                        format!("View watch `{field}.{selector}` is declared more than once"),
                    ));
                }
            }
        }
        bindings.push(binding);
    }

    Ok(bindings)
}

fn parse_parameter(
    input: &FnArg,
    component_type: &Type,
    invalidation: BindingInvalidation,
) -> syn::Result<BindingSpec> {
    let FnArg::Typed(parameter) = input else {
        return Err(syn::Error::new_spanned(
            input,
            "views cannot have a receiver; use a named binding parameter",
        ));
    };
    if let Some(attribute) = parameter.attrs.first() {
        return Err(syn::Error::new_spanned(
            attribute,
            "View binding parameters do not accept attributes",
        ));
    }
    let Pat::Ident(pattern) = &*parameter.pat else {
        return Err(syn::Error::new_spanned(
            &parameter.pat,
            "View binding parameters must use a plain identifier",
        ));
    };
    if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
        return Err(syn::Error::new_spanned(
            &parameter.pat,
            "View binding parameters must use a plain identifier",
        ));
    }
    let (role, type_role) =
        VIEW_ROLES.parse_required(&parameter.ty, "#[view]", "View parameters")?;
    let name = pattern.ident.clone();

    match role {
        RoleKind::Component => {
            parse_component(name, type_role, component_type.clone(), invalidation)
        }
        RoleKind::Model => {
            let Some(mut binding) = BindingSpec::parse_model_type_role(name, &parameter.ty)? else {
                unreachable!("the role path was checked above")
            };
            if !matches!(
                binding.source,
                BindingSource::Model(ModelSource::ViewComponentField { .. })
            ) {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    "View Model bindings project from the View component; use `model!(component.field)`",
                ));
            }
            binding.invalidation = Some(invalidation);
            Ok(binding)
        }
        RoleKind::Res => parse_resource(name, type_role),
        RoleKind::Entity => parse_capability(name, type_role, BindingCapability::Entity),
        RoleKind::Paint => parse_capability(name, type_role, BindingCapability::Paint),
        _ => unreachable!("RoleSet admitted an unsupported View role"),
    }
}

fn parse_component(
    name: Ident,
    type_role: syn::TypeMacro,
    ty: Type,
    invalidation: BindingInvalidation,
) -> syn::Result<BindingSpec> {
    type_role.mac.parse_body::<EmptyRole>().map_err(|error| {
        syn::Error::new(
            error.span(),
            "View `component!()` does not accept a type; declare `component = T` in `#[view(...)]`",
        )
    })?;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::Explicit(Box::new(ty.clone())),
        source: BindingSource::Component(ComponentSource {
            ty,
            target: ComponentTarget::CurrentEntity,
        }),
        access: BindingAccess::Read,
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: Some(invalidation),
    })
}

fn parse_resource(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let binding = parse_resource_role(name, type_role)?;
    if binding.access != BindingAccess::Read {
        return Err(syn::Error::new(
            binding.syntax.span(),
            "views cannot mutably borrow a resource",
        ));
    }
    Ok(binding)
}

fn parse_capability(
    name: Ident,
    type_role: syn::TypeMacro,
    capability: BindingCapability,
) -> syn::Result<BindingSpec> {
    type_role.mac.parse_body::<EmptyRole>()?;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::SourceDefined,
        source: BindingSource::Capability(capability),
        access: if capability == BindingCapability::Paint {
            BindingAccess::Write
        } else {
            BindingAccess::Read
        },
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: None,
    })
}

struct EmptyRole;

impl Parse for EmptyRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            Ok(Self)
        } else {
            Err(input.error("capability roles do not accept arguments"))
        }
    }
}

fn capability_name(capability: BindingCapability) -> &'static str {
    match capability {
        BindingCapability::Entity => "entity",
        BindingCapability::Paint => "paint",
        BindingCapability::Commands => "commands",
        BindingCapability::Motion => "motion",
    }
}

fn lower(function: ItemFn, args: ViewArgs, bindings: Vec<BindingSpec>) -> syn::Result<TokenStream> {
    let function_name = function.sig.ident;
    let visibility = function.vis;
    let attributes = classify_function_attributes(function.attrs, "views")?;
    let body = function.block;
    let helper_name = format_ident!(
        "__mirui_view_body_{function_name}",
        span = Span::mixed_site()
    );
    let component_type = args.component;
    let view_name = args.name.map_or_else(
        || quote!(stringify!(#function_name)),
        |literal| quote!(#literal),
    );
    let priority = ChildScope::expression(&args.priority);
    let invalidation = match args.invalidation {
        BindingInvalidation::Visual => quote!(::mirui::ui::view::ViewInvalidation::Visual),
        BindingInvalidation::Layout => quote!(::mirui::ui::view::ViewInvalidation::Layout),
    };
    let component_local = format_ident!("__mirui_component", span = Span::mixed_site());
    let has_model_projection = bindings.iter().any(|binding| {
        matches!(
            &binding.source,
            BindingSource::Model(ModelSource::ViewComponentField { .. })
        )
    });
    let mut helper_parameters = bindings
        .iter()
        .filter_map(lower_helper_parameter)
        .collect::<syn::Result<Vec<_>>>()?;
    let mut helper_arguments = bindings
        .iter()
        .filter_map(|binding| lower_helper_argument(binding, &component_local))
        .collect::<syn::Result<Vec<_>>>()?;
    if has_model_projection {
        helper_parameters.insert(0, quote!(#component_local: &#component_type));
        helper_arguments.insert(0, quote!(#component_local));
    }
    let helper_body = lower_model_reads(&bindings, &component_local, quote!(#body))?;
    let factory_component = ChildScope::ty(&component_type);
    let needs_component = bindings.iter().any(|binding| {
        matches!(
            &binding.source,
            BindingSource::Component(_)
                | BindingSource::Model(ModelSource::ViewComponentField { .. })
        )
    });
    let render_component_fetch = needs_component.then(|| {
        quote! {
            let #component_local = __mirui_world
                .get::<#factory_component>(__mirui_entity)
                .expect(concat!("View `", stringify!(#function_name), "` requires component `", stringify!(#component_type), "`"));
        }
    });
    let render_fetches = bindings
        .iter()
        .filter_map(|binding| lower_render_fetch(binding, &function_name))
        .collect::<syn::Result<Vec<_>>>()?;

    let generics = function.sig.generics;
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let factory_generics = ChildScope::generics(&generics);
    let (factory_impl_generics, _, factory_where_clause) = factory_generics.split_for_impl();
    let type_arguments = generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            syn::GenericParam::Type(parameter) => Some(&parameter.ident),
            syn::GenericParam::Const(parameter) => Some(&parameter.ident),
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect::<Vec<_>>();
    let call_generics = if type_arguments.is_empty() {
        TokenStream::new()
    } else {
        quote!(::<#(#type_arguments),*>)
    };
    let render_fn = quote!(render #call_generics);
    let helper_fn = quote!(super::#helper_name #call_generics);
    let watch_count = bindings
        .iter()
        .map(|binding| binding.watch_selectors.len())
        .sum::<usize>();
    let observation = lower_observation(&bindings, &factory_component, &factory_generics)?;
    let observe_fn = quote!(observe #call_generics);
    let with_observation =
        (watch_count != 0).then(|| quote!(.with_observation(#observe_fn, #watch_count)));
    let with_attach = args.attach.map(|path| {
        let path = ChildScope::path(&path);
        quote!(.with_attach(#path))
    });
    let with_gesture = args.gesture.map(|path| {
        let path = ChildScope::path(&path);
        quote!(.with_internal_gesture(#path))
    });
    let system_array = (!args.systems.is_empty()).then(|| {
        let systems = args.systems.iter().map(ChildScope::expression);
        quote!(const SYSTEMS: &[::mirui::ecs::System] = &[#(#systems),*];)
    });
    let with_systems = (!args.systems.is_empty()).then(|| quote!(.with_systems(SYSTEMS)));
    let FunctionAttributes {
        helper,
        declaration,
        conditional,
    } = attributes;

    Ok(quote! {
        #(#helper)*
        #[doc(hidden)]
        fn #helper_name #impl_generics (
            #(#helper_parameters),*
        ) #where_clause {
            #helper_body
        }

        #(#conditional)*
        #(#declaration)*
        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #function_name {
            #[allow(unused_imports)]
            use super::*;

            #system_array

            #[allow(unused_variables)]
            fn render #factory_impl_generics (
                __mirui_world: &::mirui::ecs::World,
                __mirui_entity: ::mirui::ecs::Entity,
                __mirui_paint: &mut ::mirui::ui::view::PaintCtx<'_>,
            ) #factory_where_clause {
                ::mirui::core::model::with_model_read_only(|| {
                    #render_component_fetch
                    #(#render_fetches)*
                    #helper_fn(#(#helper_arguments),*);
                });
            }

            #observation

            pub fn view #factory_impl_generics () -> ::mirui::ui::view::View #factory_where_clause {
                ::mirui::ui::view::View::new(#view_name, #priority, #render_fn)
                    .with_filter::<#factory_component>()
                    .with_invalidation(#invalidation)
                    #with_observation
                    #with_attach
                    #with_gesture
                    #with_systems
            }
        }
    })
}

fn lower_helper_parameter(binding: &BindingSpec) -> Option<syn::Result<TokenStream>> {
    let name = &binding.name;
    match &binding.source {
        BindingSource::Model(_) => None,
        BindingSource::Component(component) => {
            let ty = &component.ty;
            Some(Ok(quote!(#name: &#ty)))
        }
        BindingSource::Resource(resource) => {
            let ty = &resource.ty;
            Some(Ok(match binding.cardinality {
                BindingCardinality::Required => quote!(#name: &#ty),
                BindingCardinality::Optional => quote!(#name: ::core::option::Option<&#ty>),
                BindingCardinality::Many => unreachable!("resources have singular cardinality"),
            }))
        }
        BindingSource::Capability(capability) => Some(match capability {
            BindingCapability::Entity => Ok(quote!(#name: ::mirui::ecs::Entity)),
            BindingCapability::Paint => Ok(quote!(#name: &mut ::mirui::ui::view::PaintCtx<'_>)),
            _ => Err(syn::Error::new(
                binding.syntax.span(),
                "unsupported View capability binding",
            )),
        }),
        _ => Some(Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[view]`",
        ))),
    }
}

fn lower_helper_argument(
    binding: &BindingSpec,
    component_local: &Ident,
) -> Option<syn::Result<TokenStream>> {
    match &binding.source {
        BindingSource::Model(_) => None,
        BindingSource::Component(_) => Some(Ok(quote!(#component_local))),
        BindingSource::Resource(_) => {
            let local = fetched_name(&binding.name);
            Some(Ok(quote!(#local)))
        }
        BindingSource::Capability(capability) => Some(Ok(match capability {
            BindingCapability::Entity => quote!(__mirui_entity),
            BindingCapability::Paint => quote!(__mirui_paint),
            _ => {
                return Some(Err(syn::Error::new(
                    binding.syntax.span(),
                    "unsupported View capability binding",
                )));
            }
        })),
        _ => Some(Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[view]`",
        ))),
    }
}

fn lower_render_fetch(
    binding: &BindingSpec,
    function_name: &Ident,
) -> Option<syn::Result<TokenStream>> {
    match &binding.source {
        BindingSource::Component(_) => None,
        BindingSource::Resource(resource) => {
            let ty = ChildScope::ty(&resource.ty);
            let local = fetched_name(&binding.name);
            let fetch = quote!(__mirui_world.resource::<#ty>());
            Some(Ok(match binding.cardinality {
                BindingCardinality::Required => quote! {
                    let #local = #fetch.expect(concat!("View `", stringify!(#function_name), "` requires resource `", stringify!(#ty), "`"));
                },
                BindingCardinality::Optional => quote!(let #local = #fetch;),
                BindingCardinality::Many => unreachable!("resources have singular cardinality"),
            }))
        }
        BindingSource::Model(_) | BindingSource::Capability(_) => None,
        _ => Some(Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[view]`",
        ))),
    }
}

fn lower_model_reads(
    bindings: &[BindingSpec],
    component_local: &Ident,
    mut body: TokenStream,
) -> syn::Result<TokenStream> {
    for binding in bindings.iter().rev() {
        let BindingSource::Model(ModelSource::ViewComponentField { field }) = &binding.source
        else {
            continue;
        };
        let name = &binding.name;
        body = match binding.cardinality {
            BindingCardinality::Required => quote! {
                ::mirui::core::model::ModelHandle::read(&#component_local.#field, |#name| {
                    #body
                })
            },
            BindingCardinality::Optional => quote! {
                ::mirui::core::model::OptionalModelHandle::read_optional(
                    &#component_local.#field,
                    |#name| { #body },
                )
            },
            BindingCardinality::Many => {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    "View Model projections cannot have `many` cardinality",
                ));
            }
        };
    }
    Ok(body)
}

fn lower_observation(
    bindings: &[BindingSpec],
    component: &Type,
    generics: &syn::Generics,
) -> syn::Result<TokenStream> {
    let subscriptions = bindings
        .iter()
        .flat_map(|binding| {
            let source = match &binding.source {
                BindingSource::Model(ModelSource::ViewComponentField { field }) => Some(field),
                _ => None,
            };
            binding.watch_selectors.iter().map(move |selector| {
                let field = source.expect("watch selectors belong to Model projections");
                let subscribe = format_ident!("__mirui_subscribe_{selector}");
                match binding.cardinality {
                    BindingCardinality::Required => quote! {
                        __mirui_bindings.watch(|__mirui_invalidation| {
                            __mirui_component.#field.#subscribe(
                                __mirui_world,
                                __mirui_entity,
                                __mirui_invalidation,
                            )
                        });
                    },
                    BindingCardinality::Optional => quote! {
                        __mirui_bindings.watch(|__mirui_invalidation| {
                            match __mirui_component.#field.as_ref() {
                                ::core::option::Option::Some(__mirui_handle) => {
                                    __mirui_handle.#subscribe(
                                        __mirui_world,
                                        __mirui_entity,
                                        __mirui_invalidation,
                                    )
                                }
                                ::core::option::Option::None => {
                                    ::mirui::core::model::ModelSubscription::inert()
                                }
                            }
                        });
                    },
                    BindingCardinality::Many => {
                        unreachable!("View Model projections cannot have many cardinality")
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    if subscriptions.is_empty() {
        return Ok(TokenStream::new());
    }
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    Ok(quote! {
        fn observe #impl_generics (
            __mirui_world: &::mirui::ecs::World,
            __mirui_entity: ::mirui::ecs::Entity,
            __mirui_bindings: &mut ::mirui::ui::view::ViewObservationBindings,
        ) #where_clause {
            let __mirui_component = __mirui_world
                .get::<#component>(__mirui_entity)
                .expect("View observation requires its component filter");
            #(#subscriptions)*
        }
    })
}

fn fetched_name(name: &Ident) -> Ident {
    format_ident!("__mirui_resource_{name}", span = Span::mixed_site())
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    fn expand_ok(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
        expand(attr, item)
            .unwrap_or_else(|error| panic!("unexpected expansion error: {error}"))
            .to_string()
    }

    #[test]
    fn lowers_explicit_roles_without_repeating_the_raw_model_type() {
        let expanded = expand_ok(
            quote!(
                component = Board,
                name = "board",
                priority = 61,
                invalidation = Layout,
                attach = attach_board,
                gesture = board_gesture,
                systems = [tick::system()]
            ),
            quote! {
                pub fn paint_board(
                    board: component!(),
                    game: model!(component.game, watch(positions, selection)),
                    theme: res!(Theme),
                    entity: entity!(),
                    paint: paint!(),
                ) {
                    self::paint(game, theme, entity, paint);
                }
            },
        );
        assert!(expanded.contains("fn __mirui_view_body_paint_board"));
        assert!(expanded.contains("game |"));
        assert!(expanded.contains("ModelHandle :: read (& __mirui_component . game"));
        assert!(expanded.contains("__mirui_subscribe_positions"));
        assert!(expanded.contains("__mirui_subscribe_selection"));
        assert!(expanded.contains("with_observation (observe , 2usize)"));
        assert!(expanded.contains("ViewInvalidation :: Layout"));
        assert!(expanded.contains("with_filter :: < Board >"));
        assert!(expanded.contains("self :: paint"));
        assert!(expanded.contains("& mut :: mirui :: ui :: view :: PaintCtx < '_ >"));
        assert!(!expanded.contains("__mirui_renderer"));
        assert!(!expanded.contains("__mirui_rect"));
        assert!(!expanded.contains("__mirui_ctx"));
    }

    #[test]
    fn lowers_optional_model_projection_to_one_static_observation_slot() {
        let expanded = expand_ok(
            quote!(component = Board),
            quote! {
                fn paint_board(
                    game: model!(optional component.game, watch(value)),
                ) {
                    let _ = game.map(|game| game.value);
                }
            },
        );
        assert!(
            expanded.contains("OptionalModelHandle :: read_optional (& __mirui_component . game")
        );
        assert!(expanded.contains("ModelSubscription :: inert"));
        assert!(expanded.contains("with_observation (observe , 1usize)"));
    }

    #[test]
    fn rejects_duplicate_watch_across_projection_bindings() {
        let duplicate = expand(
            quote!(component = Board),
            quote! {
                fn paint_board(
                    first: model!(component.game, watch(value)),
                    second: model!(component.game, watch(value)),
                ) {}
            },
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("declared more than once"));
    }

    #[test]
    fn view_requires_component_metadata_and_projection_from_that_component() {
        let missing = expand(
            quote!(),
            quote! {
                fn paint(paint: paint!()) {}
            },
        )
        .unwrap_err();
        assert!(missing.to_string().contains("requires `component = T`"));

        let wrong_projection = expand(
            quote!(component = Board),
            quote! {
                fn paint(game: model!(other.game)) {}
            },
        )
        .unwrap_err();
        assert!(
            wrong_projection
                .to_string()
                .contains("use the reserved source `component.field`")
        );
    }

    #[test]
    fn component_data_binding_is_optional_and_does_not_repeat_the_type() {
        let marker = expand_ok(
            quote!(component = Board),
            quote! {
                fn paint(paint: paint!()) {
                    paint.set_background_handled(true);
                }
            },
        );
        assert!(marker.contains("with_filter :: < Board >"));
        assert!(!marker.contains("get :: < Board >"));

        let component = expand_ok(
            quote!(component = Board),
            quote! {
                fn paint(board: component!()) {
                    let _ = board;
                }
            },
        );
        assert!(component.contains("get :: < Board >"));

        let typed = expand(
            quote!(component = Board),
            quote! {
                fn paint(board: component!(Board)) {
                    let _ = board;
                }
            },
        )
        .unwrap_err();
        assert!(typed.to_string().contains("does not accept a type"));

        let duplicate = expand(
            quote!(component = Board, component = OtherBoard),
            quote! {
                fn paint() {}
            },
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate `component`"));
    }

    #[test]
    fn rejects_raw_parameters_and_mutable_resources() {
        let raw = expand(
            quote!(component = Board),
            quote! {
                fn paint(board: &Board) {}
            },
        )
        .unwrap_err();
        assert!(raw.to_string().contains("View parameters must use"));

        let mutable = expand(
            quote!(component = Board),
            quote! {
                fn paint(theme: res!(mut Theme)) {}
            },
        )
        .unwrap_err();
        assert!(mutable.to_string().contains("mutably borrow a resource"));
    }

    #[test]
    fn optional_resources_keep_the_shared_read_only_role_contract() {
        let expanded = expand_ok(
            quote!(component = Board),
            quote! {
                fn paint(clock: res!(optional super::Clock), theme: res!(Theme)) {
                    let _ = (clock, theme);
                }
            },
        );
        assert!(expanded.contains("clock : :: core :: option :: Option < & super :: Clock >"));
        assert!(expanded.contains("let __mirui_resource_clock = __mirui_world . resource :: < super :: super :: Clock > () ;"));
        assert!(expanded.contains("theme : & Theme"));
        assert!(expanded.contains("requires resource"));
        for role in [quote!(optional mut Theme), quote!(mut optional Theme)] {
            let error = expand(
                quote!(component = Board),
                quote!(
                    fn paint(theme: res!(#role)) {}
                ),
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("optional mutable resources are not supported")
            );
        }
    }

    #[test]
    fn rejects_duplicate_capabilities_and_parameter_attributes() {
        let duplicate = expand(
            quote!(component = Board),
            quote! {
                fn paint(first: paint!(), second: paint!()) {}
            },
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("at most one `paint!()`"));

        let attributed = expand(
            quote!(component = Board),
            quote! {
                fn paint(#[cfg(all())] board: component!()) {}
            },
        )
        .unwrap_err();
        assert!(
            attributed
                .to_string()
                .contains("binding parameters do not accept attributes")
        );
    }
}
