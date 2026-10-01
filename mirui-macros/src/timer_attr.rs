use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use std::collections::HashSet;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Expr, FnArg, Ident, ItemFn, Meta, Pat, ReturnType, Token, Type};

use crate::binding::{
    BindingAccess, BindingCapability, BindingCardinality, BindingOwnership, BindingSource,
    BindingSpec, BindingSyntax, LogicalType, ModelSource,
};

enum Schedule {
    After(Expr),
    Every(Expr),
    Repeat { count: Expr, period: Expr },
    Until { deadline: Expr, period: Expr },
}

impl Schedule {
    fn lower(&self) -> TokenStream {
        match self {
            Self::After(period) => {
                quote!(::mirui::__macro_support::TimerSchedule::after(#period))
            }
            Self::Every(period) => {
                quote!(::mirui::__macro_support::TimerSchedule::every(#period))
            }
            Self::Repeat { count, period } => {
                quote!(::mirui::__macro_support::TimerSchedule::repeat(#count, #period))
            }
            Self::Until { deadline, period } => {
                quote!(::mirui::__macro_support::TimerSchedule::until(#deadline, #period))
            }
        }
    }
}

struct TimerArgs {
    schedule: Schedule,
}

impl Parse for TimerArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut schedule = None;
        while !input.is_empty() {
            let key = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            if schedule.is_some() {
                return Err(syn::Error::new(
                    key.span(),
                    "timer schedule forms are mutually exclusive; choose exactly one of `after`, `every`, `repeat`, or `until`",
                ));
            }
            schedule = Some(match key.to_string().as_str() {
                "after" => Schedule::After(input.parse()?),
                "every" => Schedule::Every(input.parse()?),
                "repeat" => {
                    let (count, period) = parse_pair(input, "repeat", "count, period")?;
                    Schedule::Repeat { count, period }
                }
                "until" => {
                    let (deadline, period) = parse_pair(input, "until", "deadline, period")?;
                    Schedule::Until { deadline, period }
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown timer schedule; expected `after`, `every`, `repeat`, or `until`",
                    ));
                }
            });
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let schedule = schedule.ok_or_else(|| {
            input.error(
                "`#[timer]` requires exactly one schedule: `after = ms`, `every = ms`, `repeat = (count, ms)`, or `until = (deadline, ms)`",
            )
        })?;
        Ok(Self { schedule })
    }
}

fn parse_pair(input: ParseStream<'_>, schedule: &str, expected: &str) -> syn::Result<(Expr, Expr)> {
    let content;
    syn::parenthesized!(content in input);
    let first = content.parse::<Expr>()?;
    content.parse::<Token![,]>().map_err(|_| {
        syn::Error::new(
            content.span(),
            format!("`{schedule}` expects `({expected})`"),
        )
    })?;
    let second = content.parse::<Expr>()?;
    if content.peek(Token![,]) {
        content.parse::<Token![,]>()?;
    }
    if !content.is_empty() {
        return Err(content.error(format!("`{schedule}` expects exactly `({expected})`")));
    }
    Ok((first, second))
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<TimerArgs>(attr)?;
    let function = syn::parse2::<ItemFn>(item)?;
    validate_function(&function)?;
    let bindings = parse_parameters(&function.sig.inputs)?;
    lower(function, args, bindings)
}

fn validate_function(function: &ItemFn) -> syn::Result<()> {
    let signature = &function.sig;
    if let Some(token) = signature.constness {
        return Err(syn::Error::new(token.span(), "timers cannot be `const`"));
    }
    if let Some(token) = signature.asyncness {
        return Err(syn::Error::new(token.span(), "timers cannot be async"));
    }
    if let Some(token) = signature.unsafety {
        return Err(syn::Error::new(token.span(), "timers cannot be unsafe"));
    }
    if let Some(abi) = &signature.abi {
        return Err(syn::Error::new_spanned(abi, "timers cannot declare an ABI"));
    }
    if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &signature.generics,
            "timers cannot be generic",
        ));
    }
    if let Some(variadic) = &signature.variadic {
        return Err(syn::Error::new_spanned(
            variadic,
            "timers cannot be variadic",
        ));
    }
    if !matches!(signature.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &signature.output,
            "timers return `()` implicitly",
        ));
    }
    Ok(())
}

fn parse_parameters(
    inputs: &syn::punctuated::Punctuated<FnArg, Token![,]>,
) -> syn::Result<Vec<BindingSpec>> {
    let mut bindings = Vec::with_capacity(inputs.len());
    let mut names = HashSet::with_capacity(inputs.len());
    let mut entity_seen = false;
    let mut commands_seen = false;

    for input in inputs {
        let binding = parse_parameter(input)?;
        if !names.insert(binding.name.to_string()) {
            return Err(syn::Error::new(
                binding.name.span(),
                format!(
                    "timer binding `{}` is declared more than once",
                    binding.name
                ),
            ));
        }
        match &binding.source {
            BindingSource::Capability(BindingCapability::Entity) => {
                if entity_seen {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        "`#[timer]` accepts at most one `entity!()` parameter",
                    ));
                }
                entity_seen = true;
            }
            BindingSource::Capability(BindingCapability::Commands) => {
                if commands_seen {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        "`#[timer]` accepts at most one `commands!()` parameter",
                    ));
                }
                commands_seen = true;
            }
            _ => {}
        }
        bindings.push(binding);
    }
    Ok(bindings)
}

fn parse_parameter(input: &FnArg) -> syn::Result<BindingSpec> {
    let FnArg::Typed(parameter) = input else {
        return Err(syn::Error::new_spanned(
            input,
            "timers cannot have a receiver; use a named binding parameter",
        ));
    };
    if let Some(attribute) = parameter.attrs.first() {
        return Err(syn::Error::new_spanned(
            attribute,
            "timer binding parameters do not accept attributes",
        ));
    }
    let Pat::Ident(pattern) = &*parameter.pat else {
        return Err(syn::Error::new_spanned(
            &parameter.pat,
            "timer binding parameters must use a plain identifier",
        ));
    };
    if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
        return Err(syn::Error::new_spanned(
            &parameter.pat,
            "timer binding parameters must use a plain identifier",
        ));
    }
    let Type::Macro(type_role) = &*parameter.ty else {
        return Err(syn::Error::new_spanned(
            &parameter.ty,
            "timer parameters must use `model!(T)`, `entity!()`, or `commands!()`",
        ));
    };
    require_parentheses(&type_role.mac)?;
    let Some(role) = type_role.mac.path.get_ident() else {
        return Err(unknown_role_error(&type_role.mac));
    };
    let name = pattern.ident.clone();

    match role.to_string().as_str() {
        "model" => {
            let Some(binding) = BindingSpec::parse_model_type_role(name, &parameter.ty)? else {
                unreachable!("the role path was checked above")
            };
            if !matches!(
                &binding.source,
                BindingSource::Model(ModelSource::Registered { .. })
            ) || binding.cardinality != BindingCardinality::Required
            {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    "timer Model bindings are required registered instances; use `model!(T)`",
                ));
            }
            Ok(binding)
        }
        "entity" => parse_capability(name, type_role.clone(), BindingCapability::Entity),
        "commands" => parse_capability(name, type_role.clone(), BindingCapability::Commands),
        "res" | "query" | "component" | "draw" | "rect" | "view" | "motion" => {
            Err(syn::Error::new_spanned(
                &type_role.mac.path,
                format!(
                    "binding role `{role}!` is not available in `#[timer]`; expected `model!`, `entity!`, or `commands!`",
                ),
            ))
        }
        _ => Err(unknown_role_error(&type_role.mac)),
    }
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
        logical_type: LogicalType::Intrinsic(capability),
        source: BindingSource::Capability(capability),
        access: if capability == BindingCapability::Commands {
            BindingAccess::Command
        } else {
            BindingAccess::Read
        },
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership: if capability == BindingCapability::Commands {
            BindingOwnership::Deferred
        } else {
            BindingOwnership::BorrowedForCall
        },
    })
}

struct EmptyRole;

impl Parse for EmptyRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            Ok(Self)
        } else {
            Err(input.error("timer capability roles do not accept arguments"))
        }
    }
}

fn require_parentheses(role: &syn::Macro) -> syn::Result<()> {
    if matches!(role.delimiter, syn::MacroDelimiter::Paren(_)) {
        return Ok(());
    }
    Err(syn::Error::new(
        role.delimiter.span().open(),
        "timer binding roles use parentheses, for example `entity!()`",
    ))
}

fn unknown_role_error(role: &syn::Macro) -> syn::Error {
    let path = role.path.to_token_stream().to_string();
    syn::Error::new_spanned(
        &role.path,
        format!(
            "unknown timer binding role `{path}!`; expected `model!`, `entity!`, or `commands!`",
        ),
    )
}

fn lower(
    function: ItemFn,
    args: TimerArgs,
    bindings: Vec<BindingSpec>,
) -> syn::Result<TokenStream> {
    let function_name = function.sig.ident;
    let visibility = function.vis;
    let attributes = classify_function_attributes(function.attrs)?;
    let body = function.block;
    let schedule = args.schedule.lower();
    let helper_name = format_ident!(
        "__mirui_timer_body_{function_name}",
        span = Span::mixed_site()
    );
    let helper_parameters = bindings
        .iter()
        .map(lower_helper_parameter)
        .collect::<syn::Result<Vec<_>>>()?;
    let helper_arguments = bindings
        .iter()
        .map(|binding| binding.name.clone())
        .collect::<Vec<_>>();

    let mut models = Vec::new();
    let mut entity = None;
    let mut commands = None;
    for binding in &bindings {
        match &binding.source {
            BindingSource::Model(ModelSource::Registered { ty }) => {
                models.push((binding.name.clone(), ty.clone()));
            }
            BindingSource::Capability(BindingCapability::Entity) => {
                entity = Some(binding.name.clone());
            }
            BindingSource::Capability(BindingCapability::Commands) => {
                commands = Some(binding.name.clone());
            }
            _ => {
                return Err(syn::Error::new(
                    binding.syntax.span(),
                    "binding role is not available in `#[timer]`",
                ));
            }
        }
    }

    let registration_inputs = models
        .iter()
        .map(|(name, ty)| quote!(#name: <#ty as ::mirui::core::model::BindType>::Shared));
    let captures = models.iter().map(|(name, _)| {
        let captured = capture_name(name);
        quote!(let #captured = ::mirui::core::model::SharedValue::share(&#name);)
    });
    let model_aliases = models.iter().map(|(name, _)| {
        let captured = capture_name(name);
        quote!(let #name = &#captured;)
    });
    let callback_entity = internal_ident("entity");
    let deferred_commands = internal_ident("commands");
    let entity_alias = entity
        .as_ref()
        .map(|name| quote!(let #name = #callback_entity;));
    let commands_alias = commands
        .as_ref()
        .map(|name| quote!(let #name = #deferred_commands;));

    let output = if models.is_empty() {
        quote! {
            fn __run(
                #callback_entity: ::mirui::ecs::Entity,
                #deferred_commands: ::mirui::__macro_support::DeferredCommands<'_>,
            ) {
                #entity_alias
                #commands_alias
                super::#helper_name(#(#helper_arguments),*);
            }

            pub fn timer() -> ::mirui::core::timer::Timer {
                ::mirui::__macro_support::static_timer(#schedule, __run)
            }
        }
    } else {
        let first_model = &models[0].0;
        let callback = quote! {
            move |
                #callback_entity: ::mirui::ecs::Entity,
                #deferred_commands: ::mirui::__macro_support::DeferredCommands<'_>,
            | {
                #(#model_aliases)*
                #entity_alias
                #commands_alias
                super::#helper_name(#(#helper_arguments),*);
            }
        };
        let constructor = quote!(::mirui::__macro_support::bound_timer(
            #schedule,
            &#first_model,
            callback,
        ));
        let constructor = models.iter().skip(1).fold(constructor, |timer, (name, _)| {
            quote!(::mirui::__macro_support::require_timer_model(#timer, &#name))
        });
        quote! {
            pub fn timer(#(#registration_inputs),*) -> ::mirui::core::timer::Timer {
                #(#captures)*
                let callback = #callback;
                #constructor
            }
        }
    };

    let FunctionAttributes { helper, module } = attributes;
    Ok(quote! {
        #(#helper)*
        #[doc(hidden)]
        fn #helper_name(#(#helper_parameters),*) {
            #body
        }

        #(#module)*
        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #function_name {
            #[allow(unused_imports)]
            use super::*;

            #output
        }
    })
}

fn lower_helper_parameter(binding: &BindingSpec) -> syn::Result<TokenStream> {
    let name = &binding.name;
    match &binding.source {
        BindingSource::Model(ModelSource::Registered { ty }) => {
            Ok(quote!(#name: &<#ty as ::mirui::core::model::BindType>::Shared))
        }
        BindingSource::Capability(BindingCapability::Entity) => {
            Ok(quote!(#name: ::mirui::ecs::Entity))
        }
        BindingSource::Capability(BindingCapability::Commands) => Ok(quote!(
            mut #name: ::mirui::__macro_support::DeferredCommands<'_>
        )),
        _ => Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[timer]`",
        )),
    }
}

struct FunctionAttributes {
    helper: Vec<Attribute>,
    module: Vec<Attribute>,
}

fn classify_function_attributes(attributes: Vec<Attribute>) -> syn::Result<FunctionAttributes> {
    let mut helper = Vec::new();
    let mut module = Vec::new();
    for attribute in attributes {
        if attribute.path().is_ident("cfg") {
            helper.push(attribute.clone());
            module.push(attribute);
        } else if attribute.path().is_ident("cfg_attr") {
            let nested = attribute.parse_args_with(
                syn::punctuated::Punctuated::<Meta, Token![,]>::parse_terminated,
            )?;
            let mut nested = nested.into_iter();
            let Some(condition) = nested.next() else {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "`cfg_attr` requires a condition and at least one attribute",
                ));
            };
            let nested = nested.collect::<Vec<_>>();
            if nested.is_empty() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "`cfg_attr` requires at least one conditional attribute",
                ));
            }
            let mut helper_nested = Vec::new();
            let mut module_nested = Vec::new();
            for meta in nested {
                if meta.path().is_ident("cfg_attr") {
                    return Err(syn::Error::new_spanned(
                        meta,
                        "nested `cfg_attr` is not supported on timers",
                    ));
                }
                if meta.path().is_ident("cfg") {
                    helper_nested.push(meta.clone());
                    module_nested.push(meta);
                } else if meta.path().is_ident("doc") || meta.path().is_ident("deprecated") {
                    module_nested.push(meta);
                } else {
                    helper_nested.push(meta);
                }
            }
            if !helper_nested.is_empty() {
                helper.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#helper_nested),*)]
                ));
            }
            if !module_nested.is_empty() {
                module.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#module_nested),*)]
                ));
            }
        } else if attribute.path().is_ident("doc") || attribute.path().is_ident("deprecated") {
            module.push(attribute);
        } else {
            helper.push(attribute);
        }
    }
    Ok(FunctionAttributes { helper, module })
}

fn capture_name(name: &Ident) -> Ident {
    format_ident!("__mirui_model_{name}", span = Span::mixed_site())
}

fn internal_ident(name: &str) -> Ident {
    format_ident!("__mirui_{name}", span = Span::mixed_site())
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
    fn lowers_each_schedule_form() {
        let schedules = [
            (quote!(after = 10), "TimerSchedule :: after (10)"),
            (quote!(every = 20), "TimerSchedule :: every (20)"),
            (quote!(repeat = (3, 30)), "TimerSchedule :: repeat (3 , 30)"),
            (
                quote!(until = (deadline(), 40)),
                "TimerSchedule :: until (deadline () , 40)",
            ),
        ];
        for (attr, expected) in schedules {
            let output = expand_ok(
                attr,
                quote!(
                    fn tick() {}
                ),
            );
            assert!(
                output.contains(expected),
                "missing `{expected}` in `{output}`"
            );
        }
    }

    #[test]
    fn lowers_model_entity_and_commands_roles() {
        let output = expand_ok(
            quote!(every = 25),
            quote! {
                pub fn cycle(
                    game: model!(Game),
                    timer: entity!(),
                    commands: commands!(),
                ) {
                    game.advance();
                    commands.remove::<Marker>(timer);
                }
            },
        );
        assert!(output.contains("BindType > :: Shared"));
        assert!(output.contains("bound_timer"));
        assert!(output.contains("let timer = __mirui_entity"));
        assert!(output.contains("let commands = __mirui_commands"));
        assert!(!output.contains("model !"));
        assert!(!output.contains("entity !"));
        assert!(!output.contains("commands !"));
    }

    #[test]
    fn rejects_missing_or_multiple_schedule_forms() {
        let missing = expand(
            TokenStream::new(),
            quote!(
                fn tick() {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(missing.contains("requires exactly one schedule"));

        let multiple = expand(
            quote!(after = 1, every = 2),
            quote!(
                fn tick() {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(multiple.contains("mutually exclusive"));
    }

    #[test]
    fn rejects_raw_or_context_invalid_parameters() {
        let raw = expand(
            quote!(after = 1),
            quote!(
                fn tick(world: &mut World) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(raw.contains("must use `model!(T)`, `entity!()`, or `commands!()`"));

        let resource = expand(
            quote!(after = 1),
            quote!(
                fn tick(time: res!(Time)) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(resource.contains("not available in `#[timer]`"));
    }
}
