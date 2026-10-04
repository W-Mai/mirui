//! Preserve authored paths when a declaration moves into a generated child module.

use syn::visit_mut::{self, VisitMut};
use syn::{Expr, Generics, Ident, Path, Type};

pub(crate) struct ChildScope;

impl ChildScope {
    pub(crate) fn ty(original: &Type) -> Type {
        let mut ty = original.clone();
        Self.visit_type_mut(&mut ty);
        ty
    }

    pub(crate) fn expression(original: &Expr) -> Expr {
        let mut expression = original.clone();
        Self.visit_expr_mut(&mut expression);
        expression
    }

    pub(crate) fn path(original: &Path) -> Path {
        let mut path = original.clone();
        Self.visit_path_mut(&mut path);
        path
    }

    pub(crate) fn generics(original: &Generics) -> Generics {
        let mut generics = original.clone();
        Self.visit_generics_mut(&mut generics);
        generics
    }
}

impl VisitMut for ChildScope {
    fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
        let segments = ty.path.segments.len();
        visit_mut::visit_type_path_mut(self, ty);
        if let Some(qself) = &mut ty.qself
            && qself.position != 0
        {
            qself.position += ty.path.segments.len() - segments;
        }
    }

    fn visit_expr_path_mut(&mut self, expression: &mut syn::ExprPath) {
        let segments = expression.path.segments.len();
        visit_mut::visit_expr_path_mut(self, expression);
        if let Some(qself) = &mut expression.qself
            && qself.position != 0
        {
            qself.position += expression.path.segments.len() - segments;
        }
    }

    fn visit_path_mut(&mut self, path: &mut Path) {
        visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_some() {
            return;
        }
        match path.segments.first_mut() {
            Some(first) if first.ident == "self" => {
                first.ident = Ident::new("super", first.ident.span());
            }
            Some(first) if first.ident == "super" => {
                path.segments.insert(0, syn::parse_quote!(super));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn relative_types_and_constants_keep_their_authored_scope() {
        let ty: Type = syn::parse_quote! {
            (
                [self::Value; super::WIDTH],
                <self::Node as super::Project<crate::Value>>::Output,
                [u8; <self::Node as super::Project<crate::Value>>::WIDTH],
                ::core::option::Option<Imported>,
            )
        };
        let expected: Type = syn::parse_quote! {
            (
                [super::Value; super::super::WIDTH],
                <super::Node as super::super::Project<crate::Value>>::Output,
                [u8; <super::Node as super::super::Project<crate::Value>>::WIDTH],
                ::core::option::Option<Imported>,
            )
        };
        assert_eq!(
            ChildScope::ty(&ty).into_token_stream().to_string(),
            expected.into_token_stream().to_string(),
        );
        assert!(ty.into_token_stream().to_string().contains("self :: Node"));
    }

    #[test]
    fn expressions_paths_and_generic_bounds_move_without_mutating_the_original() {
        let expression: Expr =
            syn::parse_quote!(<self::Node as super::Project>::WIDTH + super::super::WIDTH);
        let expected: Expr = syn::parse_quote!(
            <super::Node as super::super::Project>::WIDTH + super::super::super::WIDTH
        );
        assert_eq!(
            ChildScope::expression(&expression)
                .into_token_stream()
                .to_string(),
            expected.into_token_stream().to_string(),
        );

        let path: Path = syn::parse_quote!(self::install);
        let expected_path: Path = syn::parse_quote!(super::install);
        assert_eq!(
            ChildScope::path(&path).into_token_stream().to_string(),
            expected_path.into_token_stream().to_string(),
        );

        let mut generics: Generics = syn::parse_quote!(<T: super::Project<self::Node>>);
        generics.where_clause = Some(syn::parse_quote!(where self::Node: super::Project));
        let mut expected: Generics = syn::parse_quote!(<T: super::super::Project<super::Node>>);
        expected.where_clause = Some(syn::parse_quote!(where super::Node: super::super::Project));
        let rebased = ChildScope::generics(&generics);
        assert_eq!(
            rebased.params.into_token_stream().to_string(),
            expected.params.into_token_stream().to_string(),
        );
        assert_eq!(
            rebased.where_clause.into_token_stream().to_string(),
            expected.where_clause.into_token_stream().to_string(),
        );
        assert!(
            generics
                .params
                .into_token_stream()
                .to_string()
                .contains("self :: Node")
        );
    }
}
