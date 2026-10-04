use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Ident, ItemFn, LitStr, ReturnType, Token, Type, bracketed};

use crate::binding::{BindingCapability, BindingSource, BindingSpec, ModelSource};
use crate::callback_lowering::{lower_fetched_parameter, lower_helper_parameter};
use crate::function_attrs::{FunctionAttributes, classify_function_attributes};

struct SystemArgs {
    name: Option<LitStr>,
    order: Ident,
    expect: Vec<Type>,
}

impl Parse for SystemArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut name = None;
        let mut order = None;
        let mut expect = None;
        while !input.is_empty() {
            let key = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "name" => {
                    if name.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `name`"));
                    }
                }
                "order" => {
                    let slot = input.parse::<Ident>().map_err(|_| {
                        syn::Error::new(
                            input.span(),
                            "`order` expects a SystemSlot name such as `Animation`",
                        )
                    })?;
                    if input.peek(Token![::]) {
                        return Err(syn::Error::new(
                            input.span(),
                            "`order` expects a bare SystemSlot name such as `Animation`",
                        ));
                    }
                    validate_slot(&slot)?;
                    if order.replace(slot).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `order`"));
                    }
                }
                "expect" => {
                    if expect.is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `expect`"));
                    }
                    let types = if input.peek(syn::token::Bracket) {
                        let content;
                        bracketed!(content in input);
                        content
                            .parse_terminated(Type::parse, Token![,])?
                            .into_iter()
                            .collect()
                    } else {
                        vec![input.parse()?]
                    };
                    if types.is_empty() {
                        return Err(syn::Error::new(key.span(), "`expect` cannot be empty"));
                    }
                    expect = Some(types);
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown `#[system]` argument; expected `name`, `order`, or `expect`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            name,
            order: order.unwrap_or_else(|| Ident::new("Normal", input.span())),
            expect: expect.unwrap_or_default(),
        })
    }
}

fn validate_slot(slot: &Ident) -> syn::Result<()> {
    match slot.to_string().as_str() {
        "SimInput" | "DeltaTime" | "InteractionState" | "Animation" | "Timer" | "ScrollInertia"
        | "LazyList" | "TabPages" | "Normal" | "PreRender" => Ok(()),
        _ => Err(syn::Error::new(
            slot.span(),
            "unknown SystemSlot; expected SimInput, DeltaTime, InteractionState, Animation, Timer, ScrollInertia, LazyList, TabPages, Normal, or PreRender",
        )),
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<SystemArgs>(attr)?;
    let function = syn::parse2::<ItemFn>(item)?;
    validate_function(&function)?;
    let bindings = BindingSpec::parse_system_parameters(&function.sig.inputs)?;
    lower(function, args, bindings)
}

fn validate_function(function: &ItemFn) -> syn::Result<()> {
    let signature = &function.sig;
    if let Some(token) = signature.constness {
        return Err(syn::Error::new(token.span(), "systems cannot be `const`"));
    }
    if let Some(token) = signature.asyncness {
        return Err(syn::Error::new(token.span(), "systems cannot be async"));
    }
    if let Some(token) = signature.unsafety {
        return Err(syn::Error::new(token.span(), "systems cannot be unsafe"));
    }
    if let Some(abi) = &signature.abi {
        return Err(syn::Error::new_spanned(
            abi,
            "systems cannot declare an ABI",
        ));
    }
    if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &signature.generics,
            "systems cannot be generic",
        ));
    }
    if let Some(variadic) = &signature.variadic {
        return Err(syn::Error::new_spanned(
            variadic,
            "systems cannot be variadic",
        ));
    }
    if !matches!(signature.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &signature.output,
            "systems return `()` implicitly",
        ));
    }
    Ok(())
}

fn lower(
    function: ItemFn,
    args: SystemArgs,
    bindings: Vec<BindingSpec>,
) -> syn::Result<TokenStream> {
    let function_name = function.sig.ident;
    let visibility = function.vis;
    let attributes = classify_function_attributes(function.attrs, "systems")?;
    let body = function.block;
    let system_name = args
        .name
        .unwrap_or_else(|| LitStr::new(&function_name.to_string(), function_name.span()));
    let slot = args.order;
    let helper_name = format_ident!(
        "__mirui_system_body_{function_name}",
        span = Span::mixed_site()
    );
    let helper_parameters = bindings
        .iter()
        .map(|binding| lower_helper_parameter(binding, "#[system]"))
        .collect::<syn::Result<Vec<_>>>()?;
    let helper_arguments = bindings
        .iter()
        .map(|binding| binding.name.clone())
        .collect::<Vec<_>>();

    let mut models = Vec::new();
    let mut fetched_names = Vec::new();
    let mut fetched_types = Vec::new();
    let mut metadata = args
        .expect
        .iter()
        .map(|ty| quote!(::mirui::__macro_support::SystemAccess::expect_component::<#ty>()))
        .collect::<Vec<_>>();
    let mut commands = None;

    for binding in &bindings {
        match &binding.source {
            BindingSource::Model(ModelSource::Registered { ty }) => {
                models.push((binding.name.clone(), ty.clone()));
            }
            BindingSource::Resource(_) | BindingSource::Query(_) => {
                let (marker, access) = lower_fetched_parameter(binding)?;
                fetched_names.push(binding.name.clone());
                fetched_types.push(marker);
                metadata.extend(access);
            }
            BindingSource::Capability(BindingCapability::Commands) => {
                commands = Some(binding.name.clone());
            }
            _ => {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    "binding role is not available in `#[system]`",
                ));
            }
        }
    }

    let registration_inputs = models
        .iter()
        .map(|(name, ty)| quote!(#name: <#ty as ::mirui::core::model::BindType>::Shared));
    let captures = models.iter().map(|(name, _)| {
        let captured = capture_name(name);
        quote!(
            let #captured = ::mirui::core::model::SharedValue::share(&#name);
        )
    });
    let model_aliases = models.iter().map(|(name, _)| {
        let captured = capture_name(name);
        quote!(let #name = &#captured;)
    });
    let world = internal_ident("world");
    let deferred_commands = internal_ident("commands");
    let fetch = lower_fetch(&fetched_names, &fetched_types, &system_name, &world);
    let commands_alias = commands
        .as_ref()
        .map(|name| quote!(let mut #name = #deferred_commands;));
    let metadata_const = quote! {
        const __ACCESS: &[::mirui::__macro_support::SystemAccess] = &[#(#metadata),*];
    };
    let has_metadata = !metadata.is_empty();
    let has_commands = commands.is_some();

    let output = if models.is_empty() {
        let callback = if has_commands {
            quote! {
                fn __run(
                    #world: &mut ::mirui::ecs::World,
                    #deferred_commands: ::mirui::__macro_support::DeferredCommands<'_>,
                ) {
                    #fetch
                    #commands_alias
                    super::#helper_name(#(#helper_arguments),*);
                }
            }
        } else {
            quote! {
                fn __run(#world: &mut ::mirui::ecs::World) {
                    #fetch
                    super::#helper_name(#(#helper_arguments),*);
                }
            }
        };
        let constructor = if has_commands {
            unsafe_constructor(quote!(::mirui::__macro_support::deferred_system(
                #system_name,
                ::mirui::ecs::SystemSlot::#slot.priority(),
                __run,
            )))
        } else {
            unsafe_constructor(quote!(::mirui::__macro_support::static_system(
                #system_name,
                ::mirui::ecs::SystemSlot::#slot.priority(),
                __run,
            )))
        };
        let constructor = with_metadata(constructor, has_metadata);
        quote! {
            #callback

            pub const fn system() -> ::mirui::ecs::System {
                #constructor
            }
        }
    } else {
        let first_model = &models[0].0;
        let callback = if has_commands {
            quote! {
                move |
                    #world: &mut ::mirui::ecs::World,
                    #deferred_commands: ::mirui::__macro_support::DeferredCommands<'_>,
                | {
                    #(#model_aliases)*
                    #fetch
                    #commands_alias
                    super::#helper_name(#(#helper_arguments),*);
                }
            }
        } else {
            quote! {
                move |#world: &mut ::mirui::ecs::World| {
                    #(#model_aliases)*
                    #fetch
                    super::#helper_name(#(#helper_arguments),*);
                }
            }
        };
        let constructor = if has_commands {
            unsafe_constructor(quote!(::mirui::__macro_support::bound_deferred_system(
                #system_name,
                ::mirui::ecs::SystemSlot::#slot.priority(),
                &#first_model,
                callback,
            )))
        } else {
            unsafe_constructor(quote!(::mirui::__macro_support::bound_system(
                #system_name,
                ::mirui::ecs::SystemSlot::#slot.priority(),
                &#first_model,
                callback,
            )))
        };
        let constructor = models.iter().skip(1).fold(constructor, |system, (name, _)| {
            quote!(::mirui::__macro_support::require_same_owner(#system, &#name))
        });
        let constructor = with_metadata(constructor, has_metadata);
        quote! {
            pub fn system(#(#registration_inputs),*) -> ::mirui::ecs::System {
                #(#captures)*
                let callback = #callback;
                #constructor
            }
        }
    };

    let FunctionAttributes {
        helper,
        declaration,
        conditional,
    } = attributes;
    Ok(quote! {
        #(#helper)*
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        fn #helper_name(#(#helper_parameters),*) {
            #body
        }

        #(#conditional)*
        #(#declaration)*
        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #function_name {
            #[allow(unused_imports)]
            use super::*;

            #metadata_const
            #output
        }
    })
}

fn unsafe_constructor(constructor: TokenStream) -> TokenStream {
    quote!({
        // SAFETY: the callback is generated from the validated binding list and
        // can access World only through the matching typed fetch operations.
        unsafe { #constructor }
    })
}

fn capture_name(name: &Ident) -> Ident {
    format_ident!("__mirui_model_{name}", span = Span::mixed_site())
}

fn internal_ident(name: &str) -> Ident {
    format_ident!("__mirui_{name}", span = Span::mixed_site())
}

fn with_metadata(constructor: TokenStream, has_metadata: bool) -> TokenStream {
    if has_metadata {
        quote! {
            // SAFETY: the generated metadata contains every typed resource and
            // component access performed by this generated callback.
            unsafe { ::mirui::__macro_support::with_metadata(#constructor, __ACCESS) }
        }
    } else {
        constructor
    }
}

fn lower_fetch(
    names: &[Ident],
    types: &[TokenStream],
    system_name: &LitStr,
    world: &Ident,
) -> TokenStream {
    if names.is_empty() {
        return TokenStream::new();
    }
    quote! {
        let (#(#names,)*) = unsafe {
            ::mirui::__macro_support::fetch_system_params::<(#(#types,)*)>(#world, #system_name)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expand_ok(attr: TokenStream, item: TokenStream) -> String {
        expand(attr, item)
            .unwrap_or_else(|error| panic!("unexpected expansion error: {error}"))
            .to_string()
    }

    #[test]
    fn lowers_every_system_role_and_static_access() {
        let output = expand_ok(
            quote!(order = Animation, expect = Active),
            quote! {
                fn tick(
                    game: model!(Game),
                    time: res!(Time),
                    settings: res!(mut Settings),
                    metrics: res!(optional Metrics),
                    rows: query!(Entity, Transform, mut Velocity, optional Label; with Active; without Hidden;),
                    commands: commands!(),
                ) {
                    game.advance(time.delta());
                    for (entity, transform, velocity, label) in rows {
                        commands.insert(entity, Done);
                    }
                }
            },
        );
        assert!(output.contains("SystemSlot :: Animation . priority"));
        assert!(output.contains("BindType > :: Shared"));
        assert!(output.contains("let game = & __mirui_model_game"));
        assert!(output.contains("Res < Time >"));
        assert!(output.contains("ResMut < Settings >"));
        assert!(output.contains("OptionalRes < Metrics >"));
        assert!(output.contains("ComponentDriver < Transform >"));
        assert!(output.contains("Write < Velocity >"));
        assert!(output.contains("OptionalRead < Label >"));
        assert!(output.contains("With < Active >"));
        assert!(output.contains("Without < Hidden >"));
        assert!(output.contains("bound_deferred_system"));
        assert!(output.contains("expect_component :: < Active >"));
        assert!(output.contains("write_component :: < Velocity >"));
        assert!(!output.contains("model !"));
        assert!(!output.contains("query !"));
    }

    #[test]
    fn no_model_system_keeps_a_const_registration_facade() {
        let output = expand_ok(
            TokenStream::new(),
            quote! {
                pub fn tick(time: res!(Time)) {
                    let _ = time;
                }
            },
        );
        assert!(output.contains("pub const fn system"));
        assert!(output.contains("static_system"));
        assert!(output.contains("SystemSlot :: Normal . priority"));
    }

    #[test]
    fn rejects_raw_world_and_non_slot_order() {
        let raw = expand(
            TokenStream::new(),
            quote!(
                fn tick(world: &mut World) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(raw.contains("must use `model!`, `res!`, `query!`, `commands!`"));

        let numeric = expand(
            quote!(order = 150),
            quote!(
                fn tick() {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(numeric.contains("expects a SystemSlot name"));

        let path = expand(
            quote!(order = SystemSlot::Animation),
            quote!(
                fn tick() {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(path.contains("expects a bare SystemSlot name"));
    }
}
