use proc_macro2::{Group, TokenStream, TokenTree};
use quote::quote;
use syn::{ExprMacro, Ident};

use crate::binding::{BindingCapability, BindingSource, BindingSpec, ComponentTarget};

pub(crate) fn fields(names: &[&str]) -> Vec<BindingSpec> {
    names
        .iter()
        .map(|name| BindingSpec::event_field(Ident::new(name, proc_macro2::Span::call_site())))
        .collect()
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
}
