use quote::quote;
use syn::visit_mut::VisitMut;

pub(crate) fn capture_output(
    output: proc_macro2::TokenStream,
    captures: Vec<syn::Ident>,
) -> proc_macro2::TokenStream {
    if captures.is_empty() {
        return output;
    }
    let mut output: syn::Expr = match syn::parse2(output) {
        Ok(output) => output,
        Err(error) => return error.to_compile_error(),
    };
    BoundCaptureRewriter { captures }.visit_expr_mut(&mut output);
    quote!(#output)
}

pub(crate) fn strip_bound_captures(
    input: proc_macro2::TokenStream,
) -> syn::Result<(Vec<syn::Ident>, Vec<syn::Ident>, proc_macro2::TokenStream)> {
    use proc_macro2::{Delimiter, TokenTree};
    let mut iter = input.clone().into_iter();
    let Some(TokenTree::Ident(marker)) = iter.next() else {
        return Ok((Vec::new(), Vec::new(), input));
    };
    if marker != "__mirui_bind" {
        return Ok((Vec::new(), Vec::new(), input));
    }
    let Some(TokenTree::Group(group)) = iter.next() else {
        return Err(syn::Error::new_spanned(marker, "expected bound names"));
    };
    if group.delimiter() != Delimiter::Parenthesis {
        return Err(syn::Error::new_spanned(group, "expected bound names"));
    }
    let names = syn::parse::Parser::parse2(
        syn::punctuated::Punctuated::<CaptureName, syn::Token![,]>::parse_terminated,
        group.stream(),
    )?;
    let Some(TokenTree::Punct(end)) = iter.next() else {
        return Err(syn::Error::new_spanned(
            marker,
            "expected `;` after bound names",
        ));
    };
    if end.as_char() != ';' {
        return Err(syn::Error::new_spanned(
            end,
            "expected `;` after bound names",
        ));
    }
    let props = names
        .iter()
        .filter(|name| name.prop)
        .map(|name| name.name.clone())
        .collect();
    Ok((
        names.into_iter().map(|name| name.name).collect(),
        props,
        iter.collect(),
    ))
}

struct CaptureName {
    name: syn::Ident,
    prop: bool,
}

impl syn::parse::Parse for CaptureName {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        if name == "prop" && input.peek(syn::token::Paren) {
            let body;
            syn::parenthesized!(body in input);
            let name = body.parse()?;
            if !body.is_empty() {
                return Err(body.error("expected one prop name"));
            }
            Ok(Self { name, prop: true })
        } else {
            Ok(Self { name, prop: false })
        }
    }
}

struct BoundCaptureRewriter {
    captures: Vec<syn::Ident>,
}

impl BoundCaptureRewriter {
    fn wrap(&self, expr: &mut syn::Expr) {
        let captures: Vec<_> = self
            .captures
            .iter()
            .filter(|name| FreeCaptureUse::contains(expr, name))
            .collect();
        if captures.is_empty() {
            return;
        }
        let original = core::mem::replace(expr, syn::parse_quote!(()));
        *expr = syn::parse_quote!({
            #(let #captures = ::mirui::core::model::SharedValue::share(&#captures);)*
            #original
        });
    }

    fn wrap_event(&self, expr: &mut syn::Expr) {
        let captures: Vec<_> = self
            .captures
            .iter()
            .filter(|name| FreeCaptureUse::contains(expr, name))
            .collect();
        if captures.is_empty() {
            return;
        }
        let owners: Vec<_> = captures
            .iter()
            .map(|name| {
                quote::format_ident!(
                    "__mirui_capture_owner_{name}",
                    span = proc_macro2::Span::mixed_site()
                )
            })
            .collect();
        let original = core::mem::replace(expr, syn::parse_quote!(()));
        let callback = syn::Ident::new("__mirui_bound_callback", proc_macro2::Span::mixed_site());
        let registered = owners.iter().fold(
            quote!(#callback),
            |value, owner| quote!(#value.require_binding(&#owner)),
        );
        *expr = syn::parse_quote!({
            #(let #owners = ::mirui::core::model::SharedValue::share(&#captures);)*
            let #callback = {
                #(let #captures = ::mirui::core::model::SharedValue::share(&#captures);)*
                #original
            };
            #registered
        });
    }
}

pub(crate) struct FreeCaptureUse<'a> {
    name: &'a syn::Ident,
    found: bool,
    shadowed: bool,
}

impl<'a> FreeCaptureUse<'a> {
    pub(crate) fn bound_by(name: &'a syn::Ident, pat: &syn::Pat) -> bool {
        Self {
            name,
            found: false,
            shadowed: false,
        }
        .pattern_binds(pat)
    }
    pub(crate) fn contains(expr: &syn::Expr, name: &'a syn::Ident) -> bool {
        use syn::visit::Visit;
        let mut visitor = Self {
            name,
            found: false,
            shadowed: false,
        };
        visitor.visit_expr(expr);
        visitor.found
    }

    fn pattern_binds(&self, pat: &syn::Pat) -> bool {
        use syn::visit::Visit;

        struct BindingFinder<'a> {
            name: &'a syn::Ident,
            found: bool,
        }

        impl<'ast> Visit<'ast> for BindingFinder<'_> {
            fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
                if pat.ident == *self.name {
                    self.found = true;
                }
                syn::visit::visit_pat_ident(self, pat);
            }
        }

        let mut finder = BindingFinder {
            name: self.name,
            found: false,
        };
        finder.visit_pat(pat);
        finder.found
    }

    fn visit_condition(&mut self, condition: &syn::Expr) -> bool {
        use syn::visit::Visit;

        match condition {
            syn::Expr::Let(let_expr) => {
                self.visit_expr(&let_expr.expr);
                self.pattern_binds(&let_expr.pat)
            }
            syn::Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
                let left_binds = self.visit_condition(&binary.left);
                let was_shadowed = self.shadowed;
                self.shadowed |= left_binds;
                let right_binds = self.visit_condition(&binary.right);
                self.shadowed = was_shadowed;
                left_binds || right_binds
            }
            _ => {
                self.visit_expr(condition);
                false
            }
        }
    }

    fn visit_format_macro(&mut self, macro_call: &syn::Macro) -> bool {
        use syn::parse::Parser;
        use syn::visit::Visit;

        let segments = &macro_call.path.segments;
        let standard_path = segments.len() == 1
            || (macro_call.path.leading_colon.is_some()
                && segments.len() == 2
                && segments.first().is_some_and(|segment| {
                    segment.ident == "std" || segment.ident == "alloc" || segment.ident == "core"
                }));
        if !standard_path
            || !segments
                .last()
                .is_some_and(|segment| segment.ident == "format" || segment.ident == "format_args")
        {
            return false;
        }
        let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        let Ok(arguments) = parser.parse2(macro_call.tokens.clone()) else {
            return false;
        };
        let Some(syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        })) = arguments.first()
        else {
            return false;
        };
        let mut explicitly_named = false;
        for argument in arguments.iter().skip(1) {
            if let syn::Expr::Assign(assign) = argument
                && let syn::Expr::Path(path) = &*assign.left
                && path.qself.is_none()
                && path.path.is_ident(self.name)
            {
                explicitly_named = true;
                self.visit_expr(&assign.right);
            } else {
                self.visit_expr(argument);
            }
        }
        if !self.shadowed
            && !explicitly_named
            && format_literal_uses_ident(&literal.value(), &self.name.to_string())
        {
            self.found = true;
        }
        true
    }
}

impl<'ast> syn::visit::Visit<'ast> for FreeCaptureUse<'_> {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if !self.shadowed && path.qself.is_none() && path.path.is_ident(self.name) {
            self.found = true;
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let was_shadowed = self.shadowed;
        for statement in &block.stmts {
            match statement {
                syn::Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        self.visit_expr(&init.expr);
                        if let Some((_, diverge)) = &init.diverge {
                            self.visit_expr(diverge);
                        }
                    }
                    self.shadowed |= self.pattern_binds(&local.pat);
                }
                syn::Stmt::Item(_) => {}
                syn::Stmt::Expr(expr, _) => self.visit_expr(expr),
                syn::Stmt::Macro(statement) => self.visit_macro(&statement.mac),
            }
        }
        self.shadowed = was_shadowed;
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let was_shadowed = self.shadowed;
        self.shadowed |= closure.inputs.iter().any(|pat| self.pattern_binds(pat));
        self.visit_expr(&closure.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_for_loop(&mut self, loop_expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&loop_expr.expr);
        let was_shadowed = self.shadowed;
        self.shadowed |= self.pattern_binds(&loop_expr.pat);
        self.visit_block(&loop_expr.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_if(&mut self, if_expr: &'ast syn::ExprIf) {
        let binds = self.visit_condition(&if_expr.cond);
        let was_shadowed = self.shadowed;
        self.shadowed |= binds;
        self.visit_block(&if_expr.then_branch);
        self.shadowed = was_shadowed;
        if let Some((_, else_branch)) = &if_expr.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, while_expr: &'ast syn::ExprWhile) {
        let binds = self.visit_condition(&while_expr.cond);
        let was_shadowed = self.shadowed;
        self.shadowed |= binds;
        self.visit_block(&while_expr.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_match(&mut self, match_expr: &'ast syn::ExprMatch) {
        self.visit_expr(&match_expr.expr);
        for arm in &match_expr.arms {
            let was_shadowed = self.shadowed;
            self.shadowed |= self.pattern_binds(&arm.pat);
            if let Some((_, guard)) = &arm.guard {
                self.visit_condition(guard);
            }
            self.visit_expr(&arm.body);
            self.shadowed = was_shadowed;
        }
    }

    fn visit_macro(&mut self, macro_call: &'ast syn::Macro) {
        use syn::parse::Parser;

        if self.visit_format_macro(macro_call) {
            return;
        }

        // `matches!` takes a pattern after its first argument.
        if macro_call
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "matches")
            && let Ok(input) = syn::parse2::<MatchesMacroInput>(macro_call.tokens.clone())
        {
            self.visit_expr(&input.value);
            if let Some(guard) = &input.guard {
                let was_shadowed = self.shadowed;
                self.shadowed |= self.pattern_binds(&input.pattern);
                self.visit_expr(guard);
                self.shadowed = was_shadowed;
            }
            return;
        }

        let arguments = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(arguments) = arguments.parse2(macro_call.tokens.clone()) {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        } else if !self.shadowed && token_stream_uses_ident(macro_call.tokens.clone(), self.name) {
            self.found = true;
        }
    }
}

fn format_literal_uses_ident(literal: &str, name: &str) -> bool {
    let bytes = literal.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'{') {
            index += 2;
            continue;
        }
        let start = index + 1;
        let Some(close) = bytes[start..].iter().position(|byte| *byte == b'}') else {
            break;
        };
        let end = start + close;
        let field = &literal[start..end];
        let argument = field.split([':', '!']).next().unwrap_or_default();
        if argument == name {
            return true;
        }
        if let Some((_, format_spec)) = field.split_once(':') {
            for (offset, _) in format_spec.match_indices(name) {
                let prefix = &format_spec[..offset];
                let suffix = &format_spec[offset + name.len()..];
                let begins_identifier = prefix
                    .chars()
                    .next_back()
                    .is_some_and(|character| character.is_alphanumeric() || character == '_');
                if !begins_identifier && suffix.starts_with('$') {
                    return true;
                }
            }
        }
        index = end + 1;
    }
    false
}

struct MatchesMacroInput {
    value: syn::Expr,
    pattern: syn::Pat,
    guard: Option<syn::Expr>,
}

impl syn::parse::Parse for MatchesMacroInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let value = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let pattern = syn::Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(syn::Token![if]) {
            input.parse::<syn::Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }
        Ok(Self {
            value,
            pattern,
            guard,
        })
    }
}

fn token_stream_uses_ident(stream: proc_macro2::TokenStream, name: &syn::Ident) -> bool {
    use proc_macro2::TokenTree;

    let tokens: Vec<_> = stream.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Ident(ident) if ident == name => {
            let is_member = index > 0
                && matches!(&tokens[index - 1], TokenTree::Punct(punct) if punct.as_char() == '.');
            let is_path = (index > 0
                && matches!(&tokens[index - 1], TokenTree::Punct(punct) if punct.as_char() == ':'))
                || tokens.get(index + 1).is_some_and(
                    |next| matches!(next, TokenTree::Punct(punct) if punct.as_char() == ':'),
                );
            !is_member && !is_path
        }
        TokenTree::Group(group) => token_stream_uses_ident(group.stream(), name),
        _ => false,
    })
}

impl syn::visit_mut::VisitMut for BoundCaptureRewriter {
    fn visit_local_mut(&mut self, local: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, local);
        if !matches!(&local.pat, syn::Pat::Ident(pattern)
            if pattern.ident == "__compose_factory")
        {
            return;
        }
        let Some(initializer) = &mut local.init else {
            return;
        };
        let syn::Expr::Call(call) = &mut *initializer.expr else {
            return;
        };
        for argument in &mut call.args {
            let syn::Expr::Path(path) = argument else {
                continue;
            };
            let Some(name) = path.path.get_ident() else {
                continue;
            };
            if self.captures.contains(name) {
                *argument = syn::parse_quote!(::mirui::core::model::SharedValue::share(&#name));
            }
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, expr);
        if let syn::Expr::MethodCall(call) = expr
            && call.method == "__mirui_prepare_field"
            && let Some(closure @ syn::Expr::Closure(_)) = call.args.last_mut()
        {
            self.wrap(closure);
        }
        let generated_event = matches!(expr, syn::Expr::MethodCall(call)
            if matches!(call.method.to_string().as_str(),
                "__bind_callback" | "__bind_plain" | "__bind_deferred" | "__bind_params" | "__bind_params_deferred")
                && matches!(&*call.receiver, syn::Expr::Path(path)
                    if path.path.is_ident("__mirui_event_key")));
        if generated_event {
            self.wrap_event(expr);
        }
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        syn::visit_mut::visit_expr_call_mut(self, call);
        let syn::Expr::Path(path) = &*call.func else {
            return;
        };
        let input_getter = path
            .path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .eq(["mirui", "ui", "Prop", "new"].map(String::from));
        if !input_getter
            && !path.path.segments.last().is_some_and(|segment| {
                segment.ident == "effect_with_widget"
                    || segment.ident == "effect_with_role"
                    || segment.ident == "prepare_widget_effect"
                    || segment.ident == "prepare_style_binding"
                    || segment.ident == "prepare_class_binding"
            })
        {
            return;
        }
        if let Some(closure @ syn::Expr::Closure(_)) = call.args.last_mut() {
            self.wrap(closure);
        }
    }
}
