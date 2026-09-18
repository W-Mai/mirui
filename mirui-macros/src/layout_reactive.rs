use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::{self, VisitMut};
use syn::{
    Block, Expr, ExprField, ExprPath, LitStr, Member, Token, braced, bracketed, parenthesized,
};

const PLACEHOLDER: &str = "__mirui_layout_value";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Axis {
    Width,
    Height,
}

impl Axis {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        match ident.to_string().as_str() {
            "width" => Ok(Self::Width),
            "height" => Ok(Self::Height),
            _ => Err(syn::Error::new(
                ident.span(),
                "layout dependency axis must be `width` or `height`",
            )),
        }
    }

    fn ident(self, span: Span) -> Ident {
        Ident::new(
            match self {
                Self::Width => "width",
                Self::Height => "height",
            },
            span,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    Nearest,
    Named(String),
}

#[derive(Clone)]
pub(crate) struct Dependency {
    pub source: Source,
    pub axis: Axis,
    pub alias: Option<Ident>,
    span: Span,
}

impl Dependency {
    fn binding_key(&self) -> BindingKey {
        if let Some(alias) = &self.alias {
            return BindingKey::Bare(alias.to_string());
        }
        match &self.source {
            Source::Nearest => BindingKey::Bare(self.axis.ident(self.span).to_string()),
            Source::Named(name) => BindingKey::Field(name.clone(), self.axis),
        }
    }
}

impl Parse for Dependency {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let first: Ident = input.parse()?;
        let span = first.span();
        let (source, axis) = if first == "id" {
            let args;
            parenthesized!(args in input);
            let name = if args.peek(LitStr) {
                args.parse::<LitStr>()?.value()
            } else {
                args.parse::<Ident>()?.to_string()
            };
            if !args.is_empty() {
                return Err(args.error("id(...) accepts one identifier or string literal"));
            }
            input.parse::<Token![.]>()?;
            let axis_ident: Ident = input.parse()?;
            (Source::Named(name), Axis::parse(&axis_ident)?)
        } else {
            (Source::Nearest, Axis::parse(&first)?)
        };
        let alias = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self {
            source,
            axis,
            alias,
            span,
        })
    }
}

#[derive(Clone)]
pub(crate) struct LayoutValue {
    pub dependencies: Vec<Dependency>,
    pub body: Block,
}

impl Parse for LayoutValue {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let deps;
        bracketed!(deps in input);
        let dependencies = deps
            .parse_terminated(Dependency::parse, Token![,])?
            .into_iter()
            .collect::<Vec<_>>();
        if dependencies.is_empty() {
            return Err(deps.error("layout binding requires at least one dependency"));
        }
        if dependencies.len() > 4 {
            return Err(deps.error("layout binding supports at most four dependencies"));
        }
        let body_content;
        let brace = braced!(body_content in input);
        let body = Block {
            brace_token: brace,
            stmts: body_content.call(Block::parse_within)?,
        };
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after layout expression"));
        }

        let mut keys = std::collections::BTreeSet::new();
        for dependency in &dependencies {
            if let Source::Named(name) = &dependency.source
                && syn::parse_str::<Ident>(name).is_err()
                && dependency.alias.is_none()
            {
                return Err(syn::Error::new(
                    dependency.span,
                    "string layout IDs require an `as alias` binding",
                ));
            }
            let key = dependency.binding_key();
            if !keys.insert(key.clone()) {
                return Err(syn::Error::new(
                    dependency.span,
                    format!("duplicate layout dependency `{}`", key.display()),
                ));
            }
        }

        Ok(Self { dependencies, body })
    }
}

impl LayoutValue {
    pub(crate) fn parse_expr(expr: &Expr) -> syn::Result<Option<Self>> {
        let Expr::Macro(expr) = expr else {
            return Ok(None);
        };
        if !expr.mac.path.is_ident(PLACEHOLDER) {
            return Ok(None);
        }
        syn::parse2(expr.mac.tokens.clone()).map(Some)
    }

    pub(crate) fn emit_apply(
        &mut self,
        property: &Ident,
        merged: &[Dependency],
    ) -> syn::Result<TokenStream> {
        let mut replacements = Vec::with_capacity(self.dependencies.len());
        let values = self
            .dependencies
            .iter()
            .map(|dependency| {
                let index = merged
                    .iter()
                    .position(|candidate| candidate.same_source(dependency))
                    .expect("merged dependency list contains every local dependency");
                let local = Ident::new(&format!("__layout_value_{index}"), dependency.span);
                replacements.push((dependency.binding_key(), local.clone()));
                quote! { let #local = __layout_values.get(#index); }
            })
            .collect::<Vec<_>>();

        let mut rewriter = BodyRewriter {
            replacements: &replacements,
            error: None,
        };
        rewriter.visit_block_mut(&mut self.body);
        if let Some(error) = rewriter.error {
            return Err(error);
        }

        let body = &self.body;
        Ok(quote! {
            #(#values)*
            #[allow(unused_braces)]
            let __layout_computed = #body;
            __layout_changed |= ::mirui::ui::property::apply_to_world::<::mirui::ui::property::prop::#property>(
                __layout_world,
                __layout_target,
                ::core::convert::Into::into(__layout_computed),
            ).changed();
        })
    }
}

impl Dependency {
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        self.source == other.source && self.axis == other.axis
    }

    pub(crate) fn emit(&self) -> TokenStream {
        let axis = match self.axis {
            Axis::Width => quote! { ::mirui::ui::LayoutAxis::Width },
            Axis::Height => quote! { ::mirui::ui::LayoutAxis::Height },
        };
        match &self.source {
            Source::Nearest => quote! { ::mirui::ui::LayoutDependency::container(#axis) },
            Source::Named(name) => quote! { ::mirui::ui::LayoutDependency::named(#name, #axis) },
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum BindingKey {
    Bare(String),
    Field(String, Axis),
}

impl BindingKey {
    fn display(&self) -> String {
        match self {
            Self::Bare(name) => name.clone(),
            Self::Field(name, Axis::Width) => format!("{name}.width"),
            Self::Field(name, Axis::Height) => format!("{name}.height"),
        }
    }
}

struct BodyRewriter<'a> {
    replacements: &'a [(BindingKey, Ident)],
    error: Option<syn::Error>,
}

impl BodyRewriter<'_> {
    fn replacement(&self, key: &BindingKey) -> Option<&Ident> {
        self.replacements
            .iter()
            .find_map(|(candidate, ident)| (candidate == key).then_some(ident))
    }

    fn reject(&mut self, span: Span, key: BindingKey) {
        if self.error.is_none() {
            self.error = Some(syn::Error::new(
                span,
                format!(
                    "layout value `{}` is not declared in the `@` dependency list",
                    key.display()
                ),
            ));
        }
    }
}

impl VisitMut for BodyRewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        if let Expr::Path(ExprPath {
            path, qself: None, ..
        }) = expr
            && let Some(ident) = path.get_ident()
        {
            let key = BindingKey::Bare(ident.to_string());
            if let Some(replacement) = self.replacement(&key).cloned() {
                *expr = syn::parse_quote_spanned! {ident.span()=> #replacement};
                return;
            }
            if matches!(ident.to_string().as_str(), "width" | "height") {
                self.reject(ident.span(), key);
                return;
            }
        }

        if let Expr::Field(ExprField {
            base,
            member: Member::Named(member),
            ..
        }) = expr
            && matches!(member.to_string().as_str(), "width" | "height")
            && let Expr::Path(ExprPath {
                path, qself: None, ..
            }) = base.as_ref()
            && let Some(source) = path.get_ident()
        {
            let axis = Axis::parse(member).expect("width/height already checked");
            let key = BindingKey::Field(source.to_string(), axis);
            if let Some(replacement) = self.replacement(&key).cloned() {
                *expr = syn::parse_quote_spanned! {member.span()=> #replacement};
            } else {
                self.reject(member.span(), key);
            }
            return;
        }
        visit_mut::visit_expr_mut(self, expr);
    }
}

pub(crate) fn preprocess(input: TokenStream) -> syn::Result<TokenStream> {
    rewrite_stream(input, false).map(|(stream, _)| stream)
}

fn rewrite_stream(input: TokenStream, allow_layout: bool) -> syn::Result<(TokenStream, bool)> {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let mut output = TokenStream::new();
    let mut changed = false;
    let mut index = 0;
    while index < tokens.len() {
        if allow_layout
            && is_at(&tokens[index])
            && let Some((consumed, replacement)) = rewrite_layout_value(&tokens[index + 1..])?
        {
            output.extend(replacement);
            changed = true;
            index += consumed + 1;
            continue;
        }

        match &tokens[index] {
            TokenTree::Group(group) => {
                let (inner, inner_changed) =
                    rewrite_stream(group.stream(), group.delimiter() == Delimiter::Parenthesis)?;
                if inner_changed {
                    let mut rewritten = Group::new(group.delimiter(), inner);
                    rewritten.set_span(group.span());
                    output.extend([TokenTree::Group(rewritten)]);
                    changed = true;
                } else {
                    output.extend([TokenTree::Group(group.clone())]);
                }
            }
            token => output.extend([token.clone()]),
        }
        index += 1;
    }
    Ok((output, changed))
}

fn is_at(token: &TokenTree) -> bool {
    matches!(token, TokenTree::Punct(punct) if punct.as_char() == '@')
}

fn rewrite_layout_value(tokens: &[TokenTree]) -> syn::Result<Option<(usize, TokenStream)>> {
    let Some(first) = tokens.first() else {
        return Ok(None);
    };
    let mut body_index = match first {
        TokenTree::Ident(ident) if ident == "width" || ident == "height" => 1,
        TokenTree::Ident(ident) if ident == "id" => {
            if tokens.len() < 5 {
                return Ok(None);
            }
            let valid = matches!(&tokens[1], TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis)
                && matches!(&tokens[2], TokenTree::Punct(p) if p.as_char() == '.')
                && matches!(&tokens[3], TokenTree::Ident(axis) if axis == "width" || axis == "height");
            if !valid {
                return Ok(None);
            }
            4
        }
        TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
            let Some(TokenTree::Group(body)) = tokens.get(1) else {
                return Ok(None);
            };
            if body.delimiter() != Delimiter::Brace {
                return Ok(None);
            }
            return Ok(Some(make_placeholder(group.stream(), body, 2)));
        }
        _ => return Ok(None),
    };
    if matches!(tokens.get(body_index), Some(TokenTree::Ident(as_token)) if as_token == "as") {
        if !matches!(tokens.get(body_index + 1), Some(TokenTree::Ident(_))) {
            return Err(syn::Error::new(
                tokens[body_index].span(),
                "layout dependency alias requires an identifier",
            ));
        }
        body_index += 2;
    }
    let Some(TokenTree::Group(body)) = tokens.get(body_index) else {
        return Ok(None);
    };
    if body.delimiter() != Delimiter::Brace {
        return Ok(None);
    }

    let dependency_tokens = tokens[..body_index].iter().cloned().collect();
    Ok(Some(make_placeholder(
        dependency_tokens,
        body,
        body_index + 1,
    )))
}

fn make_placeholder(
    dependency_tokens: TokenStream,
    body: &Group,
    consumed: usize,
) -> (usize, TokenStream) {
    let mut args = TokenStream::new();
    args.extend([TokenTree::Group(Group::new(
        Delimiter::Bracket,
        dependency_tokens,
    ))]);
    args.extend([TokenTree::Group(body.clone())]);
    let placeholder = Ident::new(PLACEHOLDER, Span::call_site());
    (consumed, quote! { #placeholder!(#args) })
}

pub(crate) fn is_layout_placeholder(expr: &Expr) -> bool {
    matches!(expr, Expr::Macro(expr) if expr.mac.path.is_ident(PLACEHOLDER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: TokenStream) -> LayoutValue {
        let rewritten = preprocess(source).unwrap();
        let root: xrune::ds_node::DsRoot = syn::parse2(rewritten).unwrap();
        let mut found = None;
        struct Capture<'a>(&'a mut Option<LayoutValue>);
        impl xrune::ds_rune::DsRune for Capture<'_> {
            fn inscribe_root(&mut self, _: &Expr) {}
            fn inscribe_widget(
                &mut self,
                _: &Ident,
                attrs: &[xrune::ds_node::ds_attr::DsAttr],
                _: &[Expr],
                _: &[xrune::ds_node::ds_on::DsOn],
                _: &[xrune::ds_node::DsTreeRef],
            ) {
                *self.0 = LayoutValue::parse_expr(&attrs[0].value).unwrap();
            }
            fn inscribe_if(
                &mut self,
                _: &Expr,
                _: bool,
                _: &[xrune::ds_node::DsTreeRef],
                _: Option<&xrune::ds_node::DsTreeRef>,
            ) {
            }
            fn inscribe_iter(
                &mut self,
                _: &Expr,
                _: &Ident,
                _: bool,
                _: Option<&Expr>,
                _: &[xrune::ds_node::DsTreeRef],
            ) {
            }
            fn inscribe_niche(&mut self, _: &Ident, _: bool, _: &[xrune::ds_node::DsTreeRef]) {}
            fn inscribe_code_block(&mut self, _: &TokenStream) {}
            fn inscribe_match(
                &mut self,
                _: &Expr,
                _: bool,
                _: &[xrune::ds_node::ds_match::DsMatchArm],
            ) {
            }
            fn seal(self) -> TokenStream {
                TokenStream::new()
            }
        }
        xrune::ds_rune::decipher::decipher(&root.get_content(), &mut Capture(&mut found));
        found.unwrap()
    }

    #[test]
    fn preprocesses_single_nearest_dependency() {
        let value = parse(quote! { View (padding: @width { width / 24 }) });
        assert_eq!(value.dependencies.len(), 1);
        assert_eq!(value.dependencies[0].source, Source::Nearest);
        assert_eq!(value.dependencies[0].axis, Axis::Width);
    }

    #[test]
    fn parses_named_multi_source_dependencies_and_aliases() {
        let value = parse(quote! {
            View (padding: @(id(stage).width, height, id("other-stage").height as other_height) {
                stage.width + height + other_height
            })
        });
        assert_eq!(value.dependencies.len(), 3);
        assert_eq!(value.dependencies[0].source, Source::Named("stage".into()));
        assert_eq!(
            value.dependencies[2].alias.as_ref().unwrap(),
            "other_height"
        );
    }

    #[test]
    fn parses_alias_on_single_dependency() {
        let value = parse(quote! {
            View (width: @id(stage).width as stage_width { stage_width })
        });
        assert_eq!(value.dependencies[0].alias.as_ref().unwrap(), "stage_width");
    }

    #[test]
    fn parses_alias_on_single_nearest_dependency() {
        let value = parse(quote! { View (width: @width as available { available }) });
        assert_eq!(value.dependencies[0].alias.as_ref().unwrap(), "available");
    }

    #[test]
    fn string_id_requires_alias() {
        let rewritten = preprocess(quote! {
            View (width: @id("curve-stage").width { 12 })
        })
        .unwrap();
        let root: xrune::ds_node::DsRoot = syn::parse2(rewritten).unwrap();
        let attr = match root.get_content().borrow().get_node() {
            xrune::ds_node::node_enum::DsNode::Widget(widget) => {
                widget.get_attrs().attrs[0].value.clone()
            }
            _ => panic!("expected widget"),
        };
        let error = match LayoutValue::parse_expr(&attr) {
            Err(error) => error,
            Ok(_) => panic!("expected invalid string id dependency"),
        };
        assert!(error.to_string().contains("require an `as alias`"));
    }

    #[test]
    fn rejects_undeclared_layout_value_in_body() {
        let mut value = parse(quote! { View (padding: @width { width.min(height) }) });
        let merged = value.dependencies.clone();
        let error = value
            .emit_apply(&Ident::new("Padding", Span::call_site()), &merged)
            .unwrap_err();
        assert!(error.to_string().contains("`height` is not declared"));
    }

    #[test]
    fn does_not_rewrite_niche_syntax() {
        let source = quote! { Card () { @header { Text("Title") } } };
        let rewritten = preprocess(source.clone()).unwrap();
        assert_eq!(rewritten.to_string(), source.to_string());
    }
}
