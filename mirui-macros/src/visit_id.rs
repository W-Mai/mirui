use syn::visit_mut::{self, VisitMut};
use syn::{Expr, ExprCall, ExprLit, ExprPath, Ident, Lit};

pub(crate) struct IdRewriter<'a> {
    pub captured: &'a mut Vec<(Ident, String)>,
}

impl VisitMut for IdRewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        if let Expr::Call(ExprCall { func, args, .. }) = expr
            && let Expr::Path(ExprPath {
                path, qself: None, ..
            }) = func.as_ref()
            && path.is_ident("id")
            && args.len() == 1
            && let Some(Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            })) = args.first()
        {
            let id_str = s.value();
            let var = Ident::new(
                &format!("__id_lookup_{}", self.captured.len()),
                proc_macro2::Span::call_site(),
            );
            *expr = syn::parse_quote! { #var };
            self.captured.push((var, id_str));
            return;
        }
        visit_mut::visit_expr_mut(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::TokenStream;
    use quote::quote;

    fn rewrite(tokens: TokenStream) -> (String, Vec<String>) {
        let mut expr: Expr = syn::parse2(tokens).unwrap();
        let mut captured = Vec::new();
        IdRewriter {
            captured: &mut captured,
        }
        .visit_expr_mut(&mut expr);
        let keys = captured.into_iter().map(|(_, s)| s).collect();
        (quote!(#expr).to_string(), keys)
    }

    #[test]
    fn captures_simple_id_call() {
        let (out, ids) = rewrite(quote!(id("hero")));
        assert_eq!(ids, vec!["hero".to_string()]);
        assert!(out.contains("__id_lookup_0"));
    }

    #[test]
    fn leaves_other_calls_alone() {
        let (out, ids) = rewrite(quote!(other("hero")));
        assert!(ids.is_empty());
        assert!(out.contains("other"));
    }

    #[test]
    fn skips_non_string_arg() {
        let (_, ids) = rewrite(quote!(id(some_var)));
        assert!(ids.is_empty());
    }

    #[test]
    fn captures_nested_id_inside_struct_literal() {
        let (out, ids) = rewrite(quote!(MirrorOf {
            target: id("src"),
            fade: 128
        }));
        assert_eq!(ids, vec!["src".to_string()]);
        assert!(out.contains("__id_lookup_0"));
        assert!(out.contains("fade"));
    }

    #[test]
    fn captures_multiple_ids_in_same_expr() {
        let (out, ids) = rewrite(quote!(Pair {
            a: id("x"),
            b: id("y")
        }));
        assert_eq!(ids, vec!["x".to_string(), "y".to_string()]);
        assert!(out.contains("__id_lookup_0"));
        assert!(out.contains("__id_lookup_1"));
    }
}
