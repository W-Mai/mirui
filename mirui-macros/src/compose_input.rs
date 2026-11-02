use proc_macro2::TokenStream;
use syn::parse::{ParseStream, Parser};
use syn::{Expr, ExprCall, ExprPath, Ident, Token};

pub(crate) fn normalize(value: &Expr, props: &[Ident]) -> Expr {
    if let Some(name) = direct_prop(value, props) {
        syn::parse_quote!(#name.get())
    } else {
        value.clone()
    }
}

pub(crate) fn direct_prop<'a>(value: &'a Expr, props: &[Ident]) -> Option<&'a Ident> {
    let value = match value {
        Expr::Block(block) if block.block.stmts.len() == 1 => {
            let syn::Stmt::Expr(value, None) = &block.block.stmts[0] else {
                return None;
            };
            value
        }
        other => other,
    };
    let Expr::Path(path) = value else {
        return None;
    };
    path.path.get_ident().filter(|name| props.contains(name))
}

pub(crate) fn getter(value: &Expr, props: &[Ident]) -> Expr {
    let value = normalize(value, props);
    let read = crate::reactive_read(&value);
    syn::parse_quote!(::mirui::ui::Prop::new(move || #read))
}

/// Parse composition calls without changing expressions inside Rust macros.
pub(crate) fn call(input: &TokenStream, props: &[Ident]) -> Option<ExprCall> {
    (|input: ParseStream<'_>| parse_call(input, props))
        .parse2(input.clone())
        .ok()
}

fn parse_call(input: ParseStream<'_>, props: &[Ident]) -> syn::Result<ExprCall> {
    let function: ExprPath = input.parse()?;
    let name = &function.path.segments.last().unwrap().ident;
    if !name
        .to_string()
        .starts_with(|character: char| character.is_ascii_lowercase() || character == '_')
    {
        return Err(syn::Error::new_spanned(
            name,
            "expected a composition function",
        ));
    }
    let arguments;
    let paren_token = syn::parenthesized!(arguments in input);
    let mut args = syn::punctuated::Punctuated::new();
    while !arguments.is_empty() {
        let value = if arguments.peek(Token![$]) {
            arguments.parse::<Token![$]>()?;
            getter(&arguments.parse::<Expr>()?, props)
        } else {
            arguments.parse()?
        };
        args.push_value(value);
        if !arguments.is_empty() {
            args.push_punct(arguments.parse()?);
        }
    }
    Ok(ExprCall {
        attrs: Vec::new(),
        func: Box::new(Expr::Path(function)),
        paren_token,
        args,
    })
}
