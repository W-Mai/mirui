use proc_macro2::{Group, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::{ExprMacro, FnArg, Ident, ItemFn, LitStr, Pat, ReturnType, Type};

use crate::binding::{
    BindingCapability, BindingSource, BindingSpec, ComponentTarget, LogicalType, MAX_FETCH_ARITY,
    ModelSource,
};
use crate::callback_lowering::{lower_fetched_parameter, lower_helper_parameter};
use crate::function_attrs::{FunctionAttributes, classify_function_attributes};

enum EventParameter {
    Binding(Box<BindingSpec>),
    Value { name: Ident, ty: Box<Type> },
}

impl EventParameter {
    fn name(&self) -> &Ident {
        match self {
            Self::Binding(binding) => &binding.name,
            Self::Value { name, .. } => name,
        }
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[event]` does not take arguments",
        ));
    }
    let function = syn::parse2::<ItemFn>(item)?;
    let signature = &function.sig;
    if signature.constness.is_some()
        || signature.asyncness.is_some()
        || signature.unsafety.is_some()
        || signature.abi.is_some()
        || signature.variadic.is_some()
        || !signature.generics.params.is_empty()
        || signature.generics.where_clause.is_some()
    {
        return Err(syn::Error::new_spanned(
            signature,
            "event callbacks use a non-generic safe synchronous function",
        ));
    }

    let mut parameters = Vec::with_capacity(signature.inputs.len());
    let mut names = HashSet::with_capacity(signature.inputs.len());
    let mut payload_seen = false;
    let mut entity_seen = false;
    let mut commands_seen = false;
    let mut fetch_count = 0;
    for input in &signature.inputs {
        let FnArg::Typed(parameter) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "event callbacks cannot have a receiver",
            ));
        };
        if let Some(attribute) = parameter.attrs.first() {
            return Err(syn::Error::new_spanned(
                attribute,
                "event parameters do not accept attributes",
            ));
        }
        let Pat::Ident(pattern) = &*parameter.pat else {
            return Err(syn::Error::new_spanned(
                &parameter.pat,
                "event parameters use plain identifiers",
            ));
        };
        if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
            return Err(syn::Error::new_spanned(
                pattern,
                "event parameters use plain identifiers",
            ));
        }
        let name = pattern.ident.clone();
        if !names.insert(name.to_string()) {
            return Err(syn::Error::new(name.span(), "duplicate event parameter"));
        }
        let Some(binding) = BindingSpec::parse_event_parameter(name.clone(), &parameter.ty)? else {
            parameters.push(EventParameter::Value {
                name,
                ty: parameter.ty.clone(),
            });
            continue;
        };
        let unique = match &binding.source {
            BindingSource::Event => Some((&mut payload_seen, "event")),
            BindingSource::Capability(BindingCapability::Entity) => {
                Some((&mut entity_seen, "entity"))
            }
            BindingSource::Capability(BindingCapability::Commands) => {
                Some((&mut commands_seen, "commands"))
            }
            BindingSource::Resource(_) | BindingSource::Query(_) => {
                if fetch_count == MAX_FETCH_ARITY {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        format!(
                            "event callbacks support at most {MAX_FETCH_ARITY} resource/query parameters"
                        ),
                    ));
                }
                fetch_count += 1;
                None
            }
            _ => None,
        };
        if let Some((seen, role)) = unique {
            if *seen {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    format!("event callbacks accept at most one `{role}!` parameter"),
                ));
            }
            *seen = true;
        }
        parameters.push(EventParameter::Binding(Box::new(binding)));
    }
    lower_callback(function, parameters)
}

fn lower_callback(function: ItemFn, parameters: Vec<EventParameter>) -> syn::Result<TokenStream> {
    let name = &function.sig.ident;
    let helper_name = format_ident!("__mirui_event_body_{name}", span = Span::mixed_site());
    let context = format_ident!("__MiruiContext", span = Span::mixed_site());
    let generic_event = format_ident!("__MiruiEvent", span = Span::mixed_site());
    let payload_name = format_ident!("__mirui_payload", span = Span::mixed_site());
    let target_name = format_ident!("__mirui_target", span = Span::mixed_site());
    let commands_name = format_ident!("__mirui_commands", span = Span::mixed_site());
    let mut helper_parameters = Vec::new();
    let mut inputs = Vec::new();
    let mut checks = Vec::new();
    let mut captures = Vec::new();
    let mut aliases = Vec::new();
    let mut models = Vec::new();
    let mut fetched_names = Vec::new();
    let mut fetched_types = Vec::new();
    let mut metadata = Vec::new();
    let mut payload_type = None;
    let mut has_entity = false;
    let mut has_commands = false;

    for parameter in &parameters {
        match parameter {
            EventParameter::Value { name, ty } => {
                helper_parameters.push(quote!(#name: #ty));
                inputs.push(quote!(#name: #ty));
                checks.push(quote!(self::__require_copy(&#name);));
            }
            EventParameter::Binding(binding) => {
                let binding_name = &binding.name;
                match &binding.source {
                    BindingSource::Event => {
                        let LogicalType::Explicit(ty) = &binding.logical_type else {
                            unreachable!("authored event parameters declare their payload type");
                        };
                        helper_parameters.push(quote!(#binding_name: &#ty));
                        aliases.push(quote!(let #binding_name = #payload_name;));
                        payload_type = Some(quote!(#ty));
                    }
                    BindingSource::Capability(BindingCapability::Entity) => {
                        helper_parameters.push(quote!(#binding_name: ::mirui::ecs::Entity));
                        aliases.push(quote!(let #binding_name = #target_name;));
                        has_entity = true;
                    }
                    BindingSource::Model(ModelSource::Registered { ty }) => {
                        helper_parameters.push(lower_helper_parameter(binding, "#[event]")?);
                        inputs.push(
                            quote!(#binding_name: <#ty as ::mirui::core::model::BindType>::Shared),
                        );
                        let capture = format_ident!(
                            "__mirui_model_{binding_name}",
                            span = Span::mixed_site()
                        );
                        captures.push(quote!(let #capture = ::mirui::core::model::SharedValue::share(&#binding_name);));
                        aliases.push(quote!(let #binding_name = &#capture;));
                        models.push(binding_name);
                    }
                    BindingSource::Resource(_) | BindingSource::Query(_) => {
                        helper_parameters.push(lower_helper_parameter(binding, "#[event]")?);
                        let (marker, access) = lower_fetched_parameter(binding)?;
                        fetched_names.push(binding_name);
                        fetched_types.push(marker);
                        metadata.extend(access);
                    }
                    BindingSource::Capability(BindingCapability::Commands) => {
                        helper_parameters.push(lower_helper_parameter(binding, "#[event]")?);
                        aliases.push(quote!(let #binding_name = #commands_name;));
                        has_commands = true;
                    }
                    _ => unreachable!("event parsing only admits event callback roles"),
                }
            }
        }
    }

    let generics = if payload_type.is_some() {
        quote!(<#context>)
    } else {
        quote!(<#generic_event, #context>)
    };
    let payload = payload_type.unwrap_or_else(|| quote!(#generic_event));
    let result = match &function.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let entity_bound = has_entity.then(|| quote!(Target = ::mirui::ecs::Entity));
    let constructor = if has_commands {
        quote!(::mirui::__macro_support::deferred_event_callback)
    } else {
        quote!(::mirui::__macro_support::event_callback)
    };
    let commands_argument = has_commands.then(|| quote!(, #commands_name));
    let arguments = parameters.iter().map(EventParameter::name);
    let callback_name = LitStr::new(&name.to_string(), name.span());
    let callback = quote! {
        // SAFETY: the generated parameter set matches the complete access plan;
        // every captured Model is checked before installation.
        unsafe {
            #constructor::<#payload, #context, (#(#fetched_types,)*), #result>(
                #callback_name,
                const { &[#(#metadata),*] },
                move |(#(#fetched_names,)*), #payload_name, #target_name #commands_argument| {
                    #(#aliases)*
                    super::#helper_name(#(#arguments),*)
                },
            )
        }
    };
    let callback = models.iter().fold(
        callback,
        |callback, model| quote!(#callback.require_model(&#model)),
    );
    let copy_check = (!checks.is_empty()).then(|| {
        quote!(
            fn __require_copy<T: ::core::marker::Copy + 'static>(_: &T) {}
        )
    });
    let visibility = &function.vis;
    let output = &function.sig.output;
    let body = &function.block;
    let FunctionAttributes { helper, module } =
        classify_function_attributes(function.attrs, "event callbacks")?;
    Ok(quote! {
        #(#helper)*
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        fn #helper_name(#(#helper_parameters),*) #output #body

        #(#module)*
        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #name {
            #[allow(unused_imports)]
            use super::*;

            #copy_check

            #[allow(clippy::too_many_arguments)]
            pub fn callback #generics (#(#inputs),*) -> ::mirui::__macro_support::EventCallback<#payload, #context>
            where
                #payload: ::mirui::input::event::contract::Event,
                #context: ::mirui::__macro_support::CallbackContext<#payload, #entity_bound>,
                #result: ::mirui::input::event::contract::EventResult<<#payload as ::mirui::input::event::contract::Event>::Route>,
            {
                #(#checks)*
                #(#captures)*
                #callback
            }
        }
    })
}

pub(crate) fn fields(names: &[&str]) -> Vec<BindingSpec> {
    names
        .iter()
        .map(|name| BindingSpec::event_field(Ident::new(name, proc_macro2::Span::call_site())))
        .collect()
}

pub(crate) fn lower_fields(bindings: &[BindingSpec]) -> TokenStream {
    let names = bindings.iter().map(|binding| &binding.name);
    quote! {
        #(let #names = *#names; let _ = #names;)*
    }
}

pub(crate) fn lower_body(body: &syn::Block, scope: &Ident) -> syn::Block {
    syn::parse2(lower_tokens(quote! { #body }, scope))
        .expect("event role lowering must preserve the Rust block grammar")
}

fn lower_tokens(input: TokenStream, scope: &Ident) -> TokenStream {
    let tokens: Vec<_> = input.into_iter().collect();
    let mut output = TokenStream::new();
    let mut cursor = 0;

    while cursor < tokens.len() {
        if let Some((end, expression)) = event_expression(&tokens, cursor) {
            match BindingSpec::parse_event_expression(expression) {
                Ok(Some(binding)) => {
                    output.extend(lower_expression(binding, scope));
                    cursor = end;
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    output.extend(error.to_compile_error());
                    cursor = end;
                    continue;
                }
            }
        }

        match &tokens[cursor] {
            TokenTree::Group(group) => {
                let mut lowered =
                    Group::new(group.delimiter(), lower_tokens(group.stream(), scope));
                lowered.set_span(group.span());
                output.extend([TokenTree::Group(lowered)]);
            }
            token => output.extend([token.clone()]),
        }
        cursor += 1;
    }

    output
}

fn event_expression(tokens: &[TokenTree], start: usize) -> Option<(usize, ExprMacro)> {
    if start > 0 {
        match &tokens[start - 1] {
            TokenTree::Ident(_) => return None,
            TokenTree::Punct(punct) if punct.as_char() == ':' => return None,
            _ => {}
        }
    }

    let mut cursor = start;
    if matches!(&tokens[cursor], TokenTree::Punct(punct) if punct.as_char() == ':') {
        if !matches!(tokens.get(cursor + 1), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
        {
            return None;
        }
        cursor += 2;
    }
    if !matches!(tokens.get(cursor), Some(TokenTree::Ident(_))) {
        return None;
    }
    cursor += 1;
    while cursor + 2 < tokens.len()
        && matches!(&tokens[cursor], TokenTree::Punct(punct) if punct.as_char() == ':')
        && matches!(&tokens[cursor + 1], TokenTree::Punct(punct) if punct.as_char() == ':')
        && matches!(&tokens[cursor + 2], TokenTree::Ident(_))
    {
        cursor += 3;
    }
    if cursor + 1 >= tokens.len()
        || !matches!(&tokens[cursor], TokenTree::Punct(punct) if punct.as_char() == '!')
        || !matches!(&tokens[cursor + 1], TokenTree::Group(_))
    {
        return None;
    }

    let end = cursor + 2;
    let expression = syn::parse2(tokens[start..end].iter().cloned().collect()).ok()?;
    Some((end, expression))
}

fn lower_expression(binding: BindingSpec, scope: &Ident) -> TokenStream {
    match binding.source {
        BindingSource::Component(component) => {
            let ComponentTarget::Expression(entity) = component.target else {
                unreachable!("event component reads retain their entity selector");
            };
            let ty = component.ty;
            let entity = lower_tokens(quote! { #entity }, scope);
            quote! { #scope.component::<#ty>(#entity) }
        }
        BindingSource::Capability(BindingCapability::Commands) => {
            quote! { #scope.commands() }
        }
        _ => unreachable!("event expressions only lower component reads and commands"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{BindingAccess, BindingCardinality, BindingSyntax, LogicalType};
    use quote::ToTokens;

    #[test]
    fn named_callback_lowers_shared_roles_and_installation_values() {
        let output = expand(
            TokenStream::new(),
            quote! {
                pub fn moved(
                    game: model!(Game), amount: u16, input: event!(DragMove), node: entity!(),
                    clock: res!(Clock), rows: query!(mut Position), commands: commands!(),
                ) -> bool {
                    game.move_by(amount, input.dx);
                    commands.remove::<Position>(node);
                    true
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(output.contains("pub fn callback < __MiruiContext >"));
        assert!(output.contains(
            "game : < Game as :: mirui :: core :: model :: BindType > :: Shared , amount : u16"
        ));
        assert!(output.contains("Target = :: mirui :: ecs :: Entity"));
        assert!(output.contains("Res < Clock >"));
        assert!(output.contains("Write < Position >"));
        assert!(output.contains("write_component :: < Position >"));
        assert!(output.contains("deferred_event_callback"));
        assert!(output.contains("require_model (& game)"));
        assert!(!output.contains("World"));
        assert!(!output.contains("event !"));
        assert!(!output.contains("query !"));
    }

    #[test]
    fn named_callback_without_payload_keeps_binding_time_event_inference() {
        let output = expand(
            TokenStream::new(),
            quote!(
                fn tick(count: res!(mut Count)) {
                    count.0 += 1;
                }
            ),
        )
        .unwrap()
        .to_string();
        assert!(output.contains("callback < __MiruiEvent , __MiruiContext >"));
        assert!(output.contains("pub fn callback < __MiruiEvent , __MiruiContext > ()"));
        assert!(output.contains("event_callback :: < __MiruiEvent"));
    }

    #[test]
    fn named_callback_rejects_duplicate_capabilities_and_unsupported_signatures() {
        for item in [
            quote!(
                fn invalid(a: event!(Tap), b: event!(Tap)) {}
            ),
            quote!(
                fn invalid(a: entity!(), b: entity!()) {}
            ),
            quote!(
                fn invalid(a: commands!(), b: commands!()) {}
            ),
            quote!(
                fn invalid(a: paint!()) {}
            ),
            quote!(
                fn invalid(a: event!()) {}
            ),
            quote!(
                fn invalid(a: model!(optional Game)) {}
            ),
            quote!(
                async fn invalid() {}
            ),
            quote!(
                fn invalid<T>() {}
            ),
        ] {
            assert!(expand(TokenStream::new(), item).is_err());
        }
        assert!(
            expand(
                quote!(Tap),
                quote!(
                    fn invalid() {}
                )
            )
            .is_err()
        );
    }

    #[test]
    fn event_fields_use_generated_shared_bindings() {
        let bindings = fields(&["x", "target"]);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].name, "x");
        assert!(matches!(bindings[0].syntax, BindingSyntax::Generated(_)));
        assert!(matches!(
            bindings[0].logical_type,
            LogicalType::SourceDefined
        ));
        assert!(matches!(bindings[0].source, BindingSource::Event));
        assert_eq!(bindings[0].access, BindingAccess::Read);
        assert_eq!(bindings[0].cardinality, BindingCardinality::Required);
    }

    #[test]
    fn event_expressions_lower_through_binding_sources() {
        let body: syn::Block = syn::parse_quote!({
            let marker = mirui::com!(target, Marker);
            commands!().insert(target, Marker(1));
            foreign::commands!();
        });
        let scope: Ident = syn::parse_quote!(__scope);
        let lowered = lower_body(&body, &scope).into_token_stream().to_string();

        assert!(lowered.contains("__scope . component :: < Marker > (target)"));
        assert!(lowered.contains("__scope . commands () . insert"));
        assert!(lowered.contains("foreign :: commands ! ()"));
    }

    #[test]
    fn nested_component_selectors_lower_every_expression_role() {
        let body: syn::Block = syn::parse_quote!({
            let marker = com!(mirui::com!(target, Parent).unwrap().0, Marker);
        });
        let scope: Ident = syn::parse_quote!(__scope);
        let lowered = lower_body(&body, &scope).into_token_stream().to_string();

        assert!(!lowered.contains("com !"));
        assert_eq!(lowered.matches("__scope . component").count(), 2);
    }
}
