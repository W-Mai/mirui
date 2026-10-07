use proc_macro2::{Group, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::{Expr, ExprMacro, FnArg, Ident, ItemFn, LitStr, Pat, ReturnType, Type};

use crate::binding::{
    BindingCapability, BindingSource, BindingSpec, LogicalType, MAX_FETCH_ARITY, ModelSource,
};
use crate::callback_lowering::{lower_fetched_parameter, lower_helper_parameter};
use crate::child_scope::ChildScope;
use crate::function_attrs::{FunctionAttributes, classify_function_attributes};

enum EventParameter {
    Binding(Box<BindingSpec>),
    Value { name: Ident, ty: Box<Type> },
}

struct InvocationParameter {
    name: Ident,
    helper: TokenStream,
    source: InvocationSource,
}

enum InvocationSource {
    Value(Box<Type>),
    Event(Box<Type>),
    Entity,
    Model { ty: Type, capture: Ident },
    Fetch { marker: Type, access: Vec<Expr> },
    Commands,
}

struct InvocationPlan {
    parameters: Vec<InvocationParameter>,
}

impl InvocationPlan {
    fn new(parameters: Vec<EventParameter>) -> syn::Result<Self> {
        let parameters = parameters
            .into_iter()
            .map(|parameter| {
                let (name, helper, source) = match parameter {
                    EventParameter::Value { name, ty } => {
                        let helper = quote!(#name: #ty);
                        (name, helper, InvocationSource::Value(ty))
                    }
                    EventParameter::Binding(binding) => {
                        let name = &binding.name;
                        let (helper, source) = match &binding.source {
                            BindingSource::Event => {
                                let LogicalType::Explicit(ty) = &binding.logical_type else {
                                    unreachable!(
                                        "authored event parameters declare their payload type"
                                    );
                                };
                                (quote!(#name: &#ty), InvocationSource::Event(ty.clone()))
                            }
                            BindingSource::Capability(BindingCapability::Entity) => (
                                quote!(#name: ::mirui::ecs::Entity),
                                InvocationSource::Entity,
                            ),
                            BindingSource::Model(ModelSource::Registered { ty }) => (
                                lower_helper_parameter(&binding, "#[event]")?,
                                InvocationSource::Model {
                                    ty: ty.clone(),
                                    capture: format_ident!(
                                        "__mirui_model_{name}",
                                        span = Span::mixed_site()
                                    ),
                                },
                            ),
                            BindingSource::Resource(_) | BindingSource::Query(_) => {
                                let (marker, access) = lower_fetched_parameter(&binding)?;
                                (
                                    lower_helper_parameter(&binding, "#[event]")?,
                                    InvocationSource::Fetch {
                                        marker: syn::parse2(marker)?,
                                        access: access
                                            .into_iter()
                                            .map(syn::parse2)
                                            .collect::<syn::Result<_>>()?,
                                    },
                                )
                            }
                            BindingSource::Capability(BindingCapability::Commands) => (
                                lower_helper_parameter(&binding, "#[event]")?,
                                InvocationSource::Commands,
                            ),
                            _ => unreachable!("event parsing only admits event callback roles"),
                        };
                        (binding.name, helper, source)
                    }
                };
                Ok(InvocationParameter {
                    name,
                    helper,
                    source,
                })
            })
            .collect::<syn::Result<_>>()?;
        Ok(Self { parameters })
    }

    fn values(&self) -> impl Iterator<Item = &Ident> {
        self.parameters.iter().filter_map(|parameter| {
            if matches!(parameter.source, InvocationSource::Value(_)) {
                Some(&parameter.name)
            } else {
                None
            }
        })
    }

    fn models(&self) -> impl Iterator<Item = (&Ident, &Ident)> {
        self.parameters.iter().filter_map(|parameter| {
            if let InvocationSource::Model { capture, .. } = &parameter.source {
                Some((&parameter.name, capture))
            } else {
                None
            }
        })
    }

    fn fetches(&self) -> impl Iterator<Item = (&Ident, &Type, &[Expr])> {
        self.parameters.iter().filter_map(|parameter| {
            if let InvocationSource::Fetch { marker, access } = &parameter.source {
                Some((&parameter.name, marker, access.as_slice()))
            } else {
                None
            }
        })
    }

    fn payload(&self) -> Option<&Type> {
        self.parameters.iter().find_map(|parameter| {
            if let InvocationSource::Event(ty) = &parameter.source {
                Some(&**ty)
            } else {
                None
            }
        })
    }

    fn has_entity(&self) -> bool {
        self.parameters
            .iter()
            .any(|parameter| matches!(parameter.source, InvocationSource::Entity))
    }

    fn has_commands(&self) -> bool {
        self.parameters
            .iter()
            .any(|parameter| matches!(parameter.source, InvocationSource::Commands))
    }

    fn commands_name(&self) -> Option<&Ident> {
        self.parameters.iter().find_map(|parameter| {
            matches!(parameter.source, InvocationSource::Commands).then_some(&parameter.name)
        })
    }

    fn add_commands(&mut self, name: Ident) -> syn::Result<()> {
        let ty: Type = syn::parse_quote!(commands!());
        let binding = BindingSpec::parse_event_parameter(name, &ty)?
            .expect("commands is an event parameter role");
        let mut command = Self::new(vec![EventParameter::Binding(Box::new(binding))])?;
        self.parameters.append(&mut command.parameters);
        Ok(())
    }

    fn aliases<'a>(
        &'a self,
        payload: &'a Ident,
        target: &'a Ident,
        commands: &'a Ident,
    ) -> impl Iterator<Item = TokenStream> + 'a {
        self.parameters.iter().filter_map(move |parameter| {
            let name = &parameter.name;
            match &parameter.source {
                InvocationSource::Event(_) => Some(quote!(let #name = #payload;)),
                InvocationSource::Entity => Some(quote!(let #name = #target;)),
                InvocationSource::Model { capture, .. } => Some(quote!(let #name = &#capture;)),
                InvocationSource::Commands => Some(quote!(let #name = #commands;)),
                InvocationSource::Value(_) | InvocationSource::Fetch { .. } => None,
            }
        })
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

    let plan = parse_invocation(signature.inputs.iter())?;
    lower_callback(function, plan)
}

fn parse_invocation<'input>(
    inputs: impl IntoIterator<Item = &'input FnArg>,
) -> syn::Result<InvocationPlan> {
    let mut parameters = Vec::new();
    let mut names = HashSet::new();
    let mut payload_seen = false;
    let mut entity_seen = false;
    let mut commands_seen = false;
    let mut fetch_count = 0;
    for input in inputs {
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
    InvocationPlan::new(parameters)
}

fn lower_callback(function: ItemFn, mut plan: InvocationPlan) -> syn::Result<TokenStream> {
    let name = &function.sig.ident;
    let helper_name = format_ident!("__mirui_event_body_{name}", span = Span::mixed_site());
    let context = format_ident!("__MiruiContext", span = Span::mixed_site());
    let generic_event = format_ident!("__MiruiEvent", span = Span::mixed_site());
    let payload_name = format_ident!("__mirui_payload", span = Span::mixed_site());
    let target_name = format_ident!("__mirui_target", span = Span::mixed_site());
    let commands_name = format_ident!("__mirui_commands", span = Span::mixed_site());
    let body_commands = plan
        .commands_name()
        .cloned()
        .unwrap_or_else(|| format_ident!("__mirui_body_commands", span = Span::mixed_site()));
    let (body, uses_commands) = lower_body(&function.block, &body_commands)?;
    if uses_commands && !plan.has_commands() {
        plan.add_commands(body_commands)?;
    }
    let helper_parameters = plan.parameters.iter().map(|parameter| &parameter.helper);
    let inputs = plan.parameters.iter().filter_map(|parameter| {
        let name = &parameter.name;
        match &parameter.source {
            InvocationSource::Value(ty) => {
                let ty = ChildScope::ty(ty);
                Some(quote!(#name: #ty))
            }
            InvocationSource::Model { ty, .. } => {
                let ty = ChildScope::ty(ty);
                Some(quote!(#name: <#ty as ::mirui::core::model::BindType>::Shared))
            }
            _ => None,
        }
    });
    let checks = plan
        .values()
        .map(|name| quote!(self::__require_copy(&#name);));
    let captures = plan.models().map(
        |(name, capture)| quote!(let #capture = ::mirui::core::model::SharedValue::share(&#name);),
    );
    let aliases = plan.aliases(&payload_name, &target_name, &commands_name);
    let fetched_names = plan.fetches().map(|(name, _, _)| name);
    let fetched_types = plan.fetches().map(|(_, marker, _)| ChildScope::ty(marker));
    let metadata = plan
        .fetches()
        .flat_map(|(_, _, access)| access)
        .map(ChildScope::expression);
    let generics = if plan.payload().is_some() {
        quote!(<#context>)
    } else {
        quote!(<#generic_event, #context>)
    };
    let payload = plan
        .payload()
        .map(ChildScope::ty)
        .map_or_else(|| quote!(#generic_event), |ty| quote!(#ty));
    let result = match &function.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => {
            let ty = ChildScope::ty(ty);
            quote!(#ty)
        }
    };
    let entity_bound = plan
        .has_entity()
        .then(|| quote!(Target = ::mirui::ecs::Entity));
    let constructor = if plan.has_commands() {
        quote!(::mirui::__macro_support::deferred_event_callback)
    } else {
        quote!(::mirui::__macro_support::event_callback)
    };
    let commands_argument = plan.has_commands().then(|| quote!(, #commands_name));
    let arguments = plan.parameters.iter().map(|parameter| &parameter.name);
    let callback_name = LitStr::new(&name.to_string(), name.span());
    let callback = quote! {
        // SAFETY: the generated parameter set matches the complete access plan;
        // all declared Model bindings are checked before installation.
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
    let callback = plan.models().fold(
        callback,
        |callback, (model, _)| quote!(#callback.require_binding(&#model)),
    );
    let copy_check = plan.values().next().map(|_| {
        quote!(
            fn __require_copy<T: ::core::marker::Copy + 'static>(_: &T) {}
        )
    });
    let visibility = &function.vis;
    let output = &function.sig.output;
    let FunctionAttributes {
        helper,
        declaration,
        conditional,
    } = classify_function_attributes(function.attrs, "event callbacks")?;
    Ok(quote! {
        #(#helper)*
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        fn #helper_name(#(#helper_parameters),*) #output #body

        #(#conditional)*
        #(#declaration)*
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
pub(crate) fn lower_inline_callback(
    expression: &syn::Expr,
    key: &Ident,
    name: &LitStr,
) -> syn::Result<TokenStream> {
    let mut literal = expression;
    loop {
        literal = match literal {
            syn::Expr::Paren(paren) if paren.attrs.is_empty() => &paren.expr,
            syn::Expr::Group(group) if group.attrs.is_empty() => &group.expr,
            _ => break,
        };
    }
    let syn::Expr::Closure(closure) = literal else {
        return Ok(quote!(#key.__bind_callback(#name, #expression)));
    };
    if closure.asyncness.is_some() || closure.constness.is_some() || closure.movability.is_some() {
        return Err(syn::Error::new_spanned(
            closure,
            "event closures are safe synchronous closures",
        ));
    }
    let mut role_closure = false;
    for input in &closure.inputs {
        if let Pat::Type(parameter) = input {
            role_closure |= BindingSpec::parse_event_parameter(
                format_ident!("__mirui_role", span = Span::mixed_site()),
                &parameter.ty,
            )?
            .is_some();
        }
    }
    let commands = format_ident!("__mirui_inline_commands", span = Span::mixed_site());
    let body: syn::Block = match &*closure.body {
        syn::Expr::Block(block) => block.block.clone(),
        body => syn::parse_quote!({ #body }),
    };
    if !role_closure {
        if closure.inputs.len() != 1 {
            return Err(syn::Error::new_spanned(
                &closure.inputs,
                "plain event closures take one borrowed payload parameter",
            ));
        }
        let (body, deferred) = lower_body(&body, &commands)?;
        let mut closure = closure.clone();
        closure.capture = Some(syn::token::Move {
            span: Span::mixed_site(),
        });
        *closure.body = syn::parse_quote!(#body);
        let method = if deferred {
            closure.inputs.push(syn::parse_quote!(mut #commands));
            quote!(__bind_deferred)
        } else {
            quote!(__bind_plain)
        };
        return Ok(quote! {
            unsafe {
                #key.#method::<::mirui::__macro_support::NodeContext, _>(#name, #closure)
            }
        });
    }
    let inputs = closure
        .inputs
        .iter()
        .map(|input| {
            let Pat::Type(parameter) = input else {
                return Err(syn::Error::new_spanned(
                    input,
                    "role event closures declare every parameter with an explicit role type",
                ));
            };
            Ok(FnArg::Typed(parameter.clone()))
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let mut plan = parse_invocation(inputs.iter())?;
    if let Some(value) = plan.values().next() {
        return Err(syn::Error::new_spanned(
            value,
            "role event closures use role parameters; ordinary values are lexical captures",
        ));
    }
    if let Some((model, _)) = plan.models().next() {
        return Err(syn::Error::new_spanned(
            model,
            "anonymous event Models use lexical captures; declare Model parameters in a named event factory",
        ));
    }
    let body_commands = plan
        .commands_name()
        .cloned()
        .unwrap_or_else(|| commands.clone());
    let (body, uses_commands) = lower_body(&body, &body_commands)?;
    if uses_commands && !plan.has_commands() {
        plan.add_commands(body_commands)?;
    }
    let payload = format_ident!("__mirui_payload", span = Span::mixed_site());
    let target = format_ident!("__mirui_target", span = Span::mixed_site());
    let payload_type = plan.payload().map(|ty| quote!(: &#ty));
    let fetched_names = plan.fetches().map(|(name, _, _)| name);
    let fetched_types = plan.fetches().map(|(_, marker, _)| marker);
    let metadata = plan.fetches().flat_map(|(_, _, access)| access);
    let aliases = plan.parameters.iter().filter_map(|parameter| {
        let name = &parameter.name;
        match &parameter.source {
            InvocationSource::Event(_) => Some(quote!(let #name = #payload;)),
            InvocationSource::Entity => Some(quote!(let #name = #target;)),
            InvocationSource::Model { .. } => unreachable!("anonymous Model roles were rejected"),
            InvocationSource::Commands => {
                Some(quote!(#[allow(unused_mut)] let mut #name = #commands;))
            }
            InvocationSource::Fetch { .. } => {
                let helper = syn::parse2::<FnArg>(parameter.helper.clone())
                    .expect("lowered helper parameter is a function argument");
                let FnArg::Typed(helper) = helper else {
                    unreachable!("typed helper parameter")
                };
                let Pat::Ident(pattern) = *helper.pat else {
                    unreachable!("plain helper parameter")
                };
                pattern
                    .mutability
                    .map(|_| quote!(#[allow(unused_mut)] let mut #name = #name;))
            }
            InvocationSource::Value(_) => unreachable!("role closure values were rejected"),
        }
    });
    let output = &closure.output;
    let commands_argument = plan.has_commands().then(|| quote!(, #commands));
    let method = if plan.has_commands() {
        quote!(__bind_params_deferred)
    } else {
        quote!(__bind_params)
    };
    let callback = quote! {
        unsafe {
            #key.#method::<::mirui::__macro_support::NodeContext, (#(#fetched_types,)*), _>(
                #name,
                const { &[#(#metadata),*] },
                move |(#(#fetched_names,)*), #payload #payload_type, #target #commands_argument| #output {
                    #(#aliases)*
                    #body
                },
            )
        }
    };
    Ok(callback)
}

pub(crate) fn lower_body(body: &syn::Block, commands: &Ident) -> syn::Result<(syn::Block, bool)> {
    let mut deferred = false;
    let tokens = lower_tokens(quote! { #body }, commands, &mut deferred)?;
    Ok((syn::parse2(tokens)?, deferred))
}

fn lower_tokens(
    input: TokenStream,
    commands: &Ident,
    deferred: &mut bool,
) -> syn::Result<TokenStream> {
    let tokens: Vec<_> = input.into_iter().collect();
    let mut output = TokenStream::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if let Some((end, expression)) = event_expression(&tokens, cursor)
            && let Some(binding) = BindingSpec::parse_event_expression(expression)?
        {
            match binding.source {
                BindingSource::Capability(BindingCapability::Commands) => {
                    *deferred = true;
                    output.extend(quote! { (&mut #commands) });
                }
                _ => unreachable!("event expression role"),
            }
            cursor = end;
            continue;
        }
        match &tokens[cursor] {
            TokenTree::Group(group) => {
                let mut lowered = Group::new(
                    group.delimiter(),
                    lower_tokens(group.stream(), commands, deferred)?,
                );
                lowered.set_span(group.span());
                output.extend([TokenTree::Group(lowered)]);
            }
            token => output.extend([token.clone()]),
        }
        cursor += 1;
    }
    Ok(output)
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
#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn invocation_plan_keeps_order_and_scope_across_factory_emission() {
        fn assert_access(expression: &Expr, operation: &str, ty: Type) {
            let Expr::Call(call) = expression else {
                panic!("access metadata must be a constructor call");
            };
            assert!(call.args.is_empty());
            let Expr::Path(function) = &*call.func else {
                panic!("access metadata must call a static constructor");
            };
            assert!(function.path.leading_colon.is_some());
            let segments: Vec<_> = function
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            assert_eq!(
                segments,
                ["mirui", "__macro_support", "SystemAccess", operation]
            );
            let syn::PathArguments::AngleBracketed(arguments) =
                &function.path.segments.last().unwrap().arguments
            else {
                panic!("access metadata must declare its accessed type");
            };
            assert_eq!(arguments.args.len(), 1);
            let syn::GenericArgument::Type(actual) = &arguments.args[0] else {
                panic!("access metadata must declare a type argument");
            };
            assert_eq!(
                actual.to_token_stream().to_string(),
                ty.to_token_stream().to_string()
            );
        }

        let function: ItemFn = syn::parse_quote! {
            fn moved(
                clock: res!(self::Clock), left: model!(self::Counter),
                input: event!(self::DragMove), amount: u16,
                rows: query!(mut super::Position), node: entity!(),
                right: model!(super::Counter), commands: commands!(),
            ) {}
        };
        let parameters = function
            .sig
            .inputs
            .iter()
            .map(|input| {
                let FnArg::Typed(parameter) = input else {
                    unreachable!();
                };
                let Pat::Ident(pattern) = &*parameter.pat else {
                    unreachable!();
                };
                let name = pattern.ident.clone();
                match BindingSpec::parse_event_parameter(name.clone(), &parameter.ty).unwrap() {
                    Some(binding) => EventParameter::Binding(Box::new(binding)),
                    None => EventParameter::Value {
                        name,
                        ty: parameter.ty.clone(),
                    },
                }
            })
            .collect();
        let plan = InvocationPlan::new(parameters).unwrap();
        let names: Vec<_> = plan
            .parameters
            .iter()
            .map(|parameter| parameter.name.to_string())
            .collect();
        assert_eq!(
            names,
            [
                "clock", "left", "input", "amount", "rows", "node", "right", "commands"
            ]
        );
        assert_eq!(
            plan.payload().unwrap().to_token_stream().to_string(),
            quote!(self::DragMove).to_string()
        );
        let fetches: Vec<_> = plan.fetches().collect();
        assert_eq!(fetches[0].0, "clock");
        assert_eq!(
            fetches[0].1.to_token_stream().to_string(),
            quote!(::mirui::__macro_support::Res<self::Clock>).to_string()
        );
        assert_access(
            &fetches[0].2[0],
            "read_resource",
            syn::parse_quote!(self::Clock),
        );
        assert_eq!(fetches[1].0, "rows");
        assert!(
            fetches[1]
                .1
                .to_token_stream()
                .to_string()
                .contains(&quote!(Write<super::Position>).to_string())
        );
        assert_access(
            &fetches[1].2[0],
            "write_component",
            syn::parse_quote!(super::Position),
        );
        assert!(plan.has_entity());
        assert!(plan.has_commands());

        let output: syn::File = syn::parse2(lower_callback(function, plan).unwrap()).unwrap();
        let syn::Item::Fn(helper) = &output.items[0] else {
            unreachable!();
        };
        let helper_parameters = helper.sig.inputs.to_token_stream().to_string();
        assert!(helper_parameters.contains(&quote!(&self::Clock).to_string()));
        assert!(helper_parameters.contains(&quote!(&self::DragMove).to_string()));
        assert!(helper_parameters.contains(&quote!(Write<super::Position>).to_string()));
        let syn::Item::Mod(factory_module) = &output.items[1] else {
            unreachable!();
        };
        let factory = factory_module
            .content
            .as_ref()
            .unwrap()
            .1
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(function) if function.sig.ident == "callback" => Some(function),
                _ => None,
            })
            .unwrap();
        let inputs: Vec<_> = factory
            .sig
            .inputs
            .iter()
            .map(|input| {
                let FnArg::Typed(parameter) = input else {
                    unreachable!();
                };
                parameter.pat.to_token_stream().to_string()
            })
            .collect();
        assert_eq!(inputs, ["left", "amount", "right"]);
        let factory_body = factory.block.to_token_stream().to_string();
        assert!(factory_body.contains("deferred_event_callback"));
        assert!(factory_body.contains(&quote!(Res<super::Clock>).to_string()));
        assert!(factory_body.contains(&quote!(Write<super::super::Position>).to_string()));
        assert!(factory_body.contains("require_binding (& left)"));
        assert!(factory_body.contains("require_binding (& right)"));
    }

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
        assert!(output.contains("require_binding (& game)"));
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
        assert!(!output.contains("deferred_event_callback"));
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
    fn event_command_expressions_share_the_deferred_capability() {
        let body: syn::Block = syn::parse_quote!({
            commands!().insert(event.target, Marker(1));
            foreign::commands!();
        });
        let commands: Ident = syn::parse_quote!(__commands);
        let (lowered, deferred) = lower_body(&body, &commands).unwrap();
        let lowered = lowered.into_token_stream().to_string();
        assert!(deferred);
        assert!(lowered.contains("& mut __commands"));
        assert!(lowered.contains("foreign :: commands ! ()"));
    }

    #[test]
    fn event_expression_lowering_leaves_unrelated_macros_untouched() {
        let body: syn::Block = syn::parse_quote!({
            foreign::capability!(event.target);
        });
        let (lowered, deferred) = lower_body(&body, &syn::parse_quote!(__commands)).unwrap();
        assert!(!deferred);
        assert_eq!(quote!(#lowered).to_string(), quote!(#body).to_string());
    }

    #[test]
    fn inline_role_closures_share_the_named_invocation_plan_without_child_scope_relocation() {
        let expression =
            syn::parse_quote!(|input: event!(super::Hit),
                               count: res!(mut Stats),
                               rows: query!(Entity, mut Position),
                               node: entity!(),
                               pending: commands!()| {
                pending.insert(node, Position(1));
                count.0 += input.value;
                game.increment();
                for (_, position) in rows {
                    position.0 += 1;
                }
            });
        let output = lower_inline_callback(
            &expression,
            &syn::parse_quote!(__mirui_event_key),
            &syn::parse_quote!("inline"),
        )
        .unwrap()
        .to_string();
        assert!(output.contains("__bind_params_deferred"));
        assert!(output.contains("ResMut < Stats >"));
        assert!(output.contains("game . increment"));
        assert!(output.contains("& super :: Hit"));
        assert!(!output.contains("super :: super :: Hit"));
        assert!(!output.contains("World"));
        assert!(!output.contains("event !"));
        assert!(!output.contains("query !"));
        assert!(syn::parse2::<syn::Expr>(output.parse().unwrap()).is_ok());
    }

    #[test]
    fn literal_payload_closures_keep_the_direct_expected_type_anchor() {
        let expression = syn::parse_quote!(|input| input.x > Fixed::ZERO);
        let output = lower_inline_callback(
            &expression,
            &syn::parse_quote!(__mirui_event_key),
            &syn::parse_quote!("plain"),
        )
        .unwrap()
        .to_string();
        assert!(output.contains("__bind_plain"));
        assert!(output.contains("move | input |"));
        assert!(!output.contains("__bind_params"));
        let grouped = syn::parse_quote!((|input| input.x > Fixed::ZERO));
        let grouped = lower_inline_callback(
            &grouped,
            &syn::parse_quote!(__mirui_event_key),
            &syn::parse_quote!("grouped"),
        )
        .unwrap()
        .to_string();
        assert!(grouped.contains("__bind_plain"));
        for value in [
            syn::parse_quote!(handler::callback(game.clone())),
            syn::parse_quote!(fire),
        ] {
            assert_eq!(
                lower_inline_callback(
                    &value,
                    &syn::parse_quote!(__mirui_event_key),
                    &syn::parse_quote!("value")
                )
                .unwrap()
                .to_string(),
                quote!(__mirui_event_key.__bind_callback("value", #value)).to_string()
            );
        }
    }

    #[test]
    fn inline_roles_reject_partial_ordinary_and_guessed_model_parameter_lists() {
        for expression in [
            syn::parse_quote!(|input, state: res!(State)| {}),
            syn::parse_quote!(|value: u32, state: res!(State)| {}),
            syn::parse_quote!(|game: model!(Game)| {}),
            syn::parse_quote!(|first: event!(Hit), second: event!(Hit)| {}),
        ] {
            assert!(
                lower_inline_callback(
                    &expression,
                    &syn::parse_quote!(__mirui_event_key),
                    &syn::parse_quote!("invalid")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn inline_roles_share_parameter_attributes_and_fetch_arity_validation() {
        let expression: syn::Expr = syn::parse_quote!(|#[allow(unused)] input: event!(Hit)| {});
        assert!(
            lower_inline_callback(
                &expression,
                &syn::parse_quote!(__mirui_event_key),
                &syn::parse_quote!("invalid")
            )
            .unwrap_err()
            .to_string()
            .contains("do not accept attributes")
        );
        let parameters: Vec<_> = (0..9)
            .map(|index| {
                let name = format_ident!("value_{index}");
                quote!(#name: res!(Clock))
            })
            .collect();
        let expression = syn::parse2(quote!(|#(#parameters),*| {})).unwrap();
        assert!(
            lower_inline_callback(
                &expression,
                &syn::parse_quote!(__mirui_event_key),
                &syn::parse_quote!("invalid")
            )
            .unwrap_err()
            .to_string()
            .contains("at most 8")
        );
    }
}
