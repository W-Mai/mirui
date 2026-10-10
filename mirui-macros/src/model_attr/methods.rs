use super::returns_canonical_result;
use proc_macro2::TokenStream;
use quote::quote;
use syn::visit_mut::{self, VisitMut};
use syn::{FnArg, GenericParam, Generics, ImplItemFn, Pat, Path, Type, TypePath, parse_quote};

#[derive(Clone, Copy)]
pub(super) enum Facade {
    Registered,
    Owned,
}

pub(super) fn method(
    method: &ImplItemFn,
    raw: &TypePath,
    facade: Facade,
) -> syn::Result<TokenStream> {
    let mut signature = method.sig.clone();
    let Some(FnArg::Receiver(receiver)) = signature.inputs.first() else {
        return Err(syn::Error::new_spanned(
            &signature,
            "model forwarding requires a receiver",
        ));
    };
    let mutable = receiver.mutability.is_some();
    let mut argument_names = Vec::new();
    let mut lowering = LowerSelf { raw };
    for argument in signature.inputs.iter_mut().skip(1) {
        let FnArg::Typed(argument) = argument else {
            return Err(syn::Error::new_spanned(argument, "unexpected receiver"));
        };
        let Pat::Ident(binding) = &*argument.pat else {
            return Err(syn::Error::new_spanned(
                &argument.pat,
                "model method arguments must use names",
            ));
        };
        let name = binding.ident.clone();
        *argument.pat = parse_quote!(#name);
        argument_names.push(name);
        lowering.visit_type_mut(&mut argument.ty);
    }
    lowering.visit_generics_mut(&mut signature.generics);
    lowering.visit_return_type_mut(&mut signature.output);
    if matches!(facade, Facade::Registered) {
        signature.inputs[0] = parse_quote!(&self);
    }

    let generics = signature
        .generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Type(parameter) => Some(&parameter.ident),
            GenericParam::Const(parameter) => Some(&parameter.ident),
            GenericParam::Lifetime(_) => None,
        });
    let generic_arguments: Vec<_> = generics.collect();
    let turbofish = if generic_arguments.is_empty() {
        quote!()
    } else {
        quote!(::<#(#generic_arguments),*>)
    };
    let name = &signature.ident;
    let call = quote!(data.#name #turbofish (#(#argument_names),*));
    let protocol = match facade {
        Facade::Registered => quote!(::mirui::core::model::ModelHandle),
        Facade::Owned => quote!(::mirui::core::model::OwnedModel),
    };
    let invoke = if mutable && returns_canonical_result(&signature.output) {
        quote!(#protocol::try_update(self, |data| #call))
    } else if mutable {
        quote!(#protocol::update(self, |data| #call))
    } else {
        quote!(#protocol::read(self, |data| #call))
    };
    let attributes = &method.attrs;
    let visibility = &method.vis;
    Ok(quote! {
        #(#attributes)*
        #visibility #signature {
            #invoke
        }
    })
}

pub(super) fn lower_type(ty: &Type, raw: &TypePath) -> Type {
    let mut ty = ty.clone();
    LowerSelf { raw }.visit_type_mut(&mut ty);
    ty
}

pub(super) fn lower_generics(generics: &Generics, raw: &TypePath) -> Generics {
    let mut generics = generics.clone();
    LowerSelf { raw }.visit_generics_mut(&mut generics);
    generics
}

struct LowerSelf<'a> {
    raw: &'a TypePath,
}

impl VisitMut for LowerSelf<'_> {
    fn visit_type_path_mut(&mut self, ty: &mut TypePath) {
        visit_mut::visit_type_path_mut(self, ty);
        if ty.qself.is_some() || !starts_with_self(&ty.path) {
            return;
        }
        let raw = self.raw;
        if ty.path.segments.len() == 1 {
            *ty = raw.clone();
        } else {
            let suffix = suffix(&ty.path);
            *ty = parse_quote!(<#raw>::#suffix);
        }
    }

    fn visit_expr_path_mut(&mut self, expression: &mut syn::ExprPath) {
        visit_mut::visit_expr_path_mut(self, expression);
        if expression.qself.is_some() || !starts_with_self(&expression.path) {
            return;
        }
        let raw = self.raw;
        if expression.path.segments.len() == 1 {
            expression.path = raw.path.clone();
            for segment in &mut expression.path.segments {
                if let syn::PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                    arguments.colon2_token = Some(Default::default());
                }
            }
        } else {
            let suffix = suffix(&expression.path);
            let mut lowered: syn::ExprPath = parse_quote!(<#raw>::#suffix);
            lowered.attrs = core::mem::take(&mut expression.attrs);
            *expression = lowered;
        }
    }
}

fn starts_with_self(path: &Path) -> bool {
    path.leading_colon.is_none()
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self")
}

fn suffix(path: &Path) -> Path {
    Path {
        leading_colon: None,
        segments: path.segments.iter().skip(1).cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Facade, lower_generics, lower_type, method};
    use quote::quote;
    use syn::{FnArg, Generics, ImplItemFn, Pat, Type, TypePath, parse_quote};

    fn signature(method: &ImplItemFn) -> String {
        let signature = &method.sig;
        quote!(#signature).to_string()
    }

    #[test]
    fn forwards_original_self_types_and_type_generics_through_both_facades() {
        let raw: TypePath = parse_quote!(super::Value<T>);
        let authored: ImplItemFn = parse_quote! {
            pub(super) fn map_copy<U>(&self, map: impl FnOnce(Self) -> U) -> U
            where
                Self: Clone,
                U: From<Option<Self>>,
            {
                map(self.duplicate())
            }
        };
        let expected: ImplItemFn = parse_quote! {
            fn map_copy<U>(&self, map: impl FnOnce(super::Value<T>) -> U) -> U
            where
                super::Value<T>: Clone,
                U: From<Option<super::Value<T>>>,
            {}
        };
        for facade in [Facade::Registered, Facade::Owned] {
            let forwarded: ImplItemFn =
                syn::parse2(method(&authored, &raw, facade).unwrap()).unwrap();
            assert_eq!(signature(&forwarded), signature(&expected));
            assert_eq!(
                quote!(#forwarded)
                    .to_string()
                    .matches("map_copy :: < U >")
                    .count(),
                1
            );
        }
        assert_eq!(signature(&authored).matches("Self").count(), 3);
    }

    #[test]
    fn owned_mutation_keeps_a_mutable_receiver_and_normalizes_argument_bindings() {
        let raw: TypePath = parse_quote!(Value<T>);
        let authored: ImplItemFn = parse_quote! {
            #[cfg(all())]
            pub fn replace(&mut self, mut other: Self, ref spare: Option<Self>) {
                self.value = other.value;
            }
        };
        for facade in [Facade::Registered, Facade::Owned] {
            let forwarded: ImplItemFn =
                syn::parse2(method(&authored, &raw, facade).unwrap()).unwrap();
            let FnArg::Receiver(receiver) = &forwarded.sig.inputs[0] else {
                panic!("receiver")
            };
            assert_eq!(
                receiver.mutability.is_some(),
                matches!(facade, Facade::Owned)
            );
            for argument in forwarded.sig.inputs.iter().skip(1) {
                let FnArg::Typed(argument) = argument else {
                    panic!("typed argument")
                };
                let Pat::Ident(binding) = &*argument.pat else {
                    panic!("named binding")
                };
                assert!(binding.by_ref.is_none());
                assert!(binding.mutability.is_none());
                assert!(binding.subpat.is_none());
            }
            let text = quote!(#forwarded).to_string();
            assert!(text.contains(":: update"));
            assert!(text.contains("Value < T >"));
            assert_eq!(forwarded.attrs.len(), 1);
        }
    }

    #[test]
    fn fallible_mutation_uses_the_common_result_classifier() {
        let raw: TypePath = parse_quote!(Value);
        let authored: ImplItemFn = parse_quote! {
            fn attempt(&mut self) -> (::core::result::Result<(), Error>) {
                Ok(())
            }
        };
        for facade in [Facade::Registered, Facade::Owned] {
            let text = method(&authored, &raw, facade).unwrap().to_string();
            assert!(text.contains(":: try_update"));
        }
    }

    #[test]
    fn explicitly_forwards_type_and_const_generics_but_not_lifetimes() {
        let raw: TypePath = parse_quote!(Value);
        let authored: ImplItemFn = parse_quote! {
            fn map<'a, T, const N: usize>(&self, input: &'a T) -> [u8; N] {
                [0; N]
            }
        };
        for facade in [Facade::Registered, Facade::Owned] {
            let text = method(&authored, &raw, facade).unwrap().to_string();
            assert!(text.contains("data . map :: < T , N > (input)"));
            assert!(!text.contains("map :: < 'a"));
        }
    }

    #[test]
    fn recursively_lowers_qualified_paths_functions_tuples_and_const_expressions() {
        let raw: TypePath = parse_quote!(super::Value<T>);
        let input: Type = parse_quote! {
            (
                Option<Self>,
                <Self as Trait<Self>>::Item,
                Self::Item,
                fn(Self) -> Option<Self>,
                [u8; Self::N],
            )
        };
        let expected: Type = parse_quote! {
            (
                Option<super::Value<T>>,
                <super::Value<T> as Trait<super::Value<T>>>::Item,
                <super::Value<T>>::Item,
                fn(super::Value<T>) -> Option<super::Value<T>>,
                [u8; <super::Value<T>>::N],
            )
        };
        let actual = lower_type(&input, &raw);
        assert_eq!(quote!(#actual).to_string(), quote!(#expected).to_string());
    }

    #[test]
    fn outer_generics_keep_defaults_and_lower_all_raw_self_constraints() {
        let raw: TypePath = parse_quote!(Value<T>);
        let input: syn::ItemStruct = parse_quote! {
            struct Parameters<T = Self, const N: usize = { Self::N }>
            where Self: Clone, T: Into<Self> {
                item: T,
            }
        };
        let expected: syn::ItemStruct = parse_quote! {
            struct Parameters<T = Value<T>, const N: usize = { <Value<T>>::N }>
            where Value<T>: Clone, T: Into<Value<T>> {
                item: T,
            }
        };
        let actual = lower_generics(&input.generics, &raw);
        fn render(generics: &Generics) -> String {
            let where_clause = &generics.where_clause;
            quote!(#generics #where_clause).to_string()
        }
        assert_eq!(render(&actual), render(&expected.generics));
        let (implementation, _, _) = actual.split_for_impl();
        let implementation = quote!(#implementation).to_string();
        assert!(!implementation.contains('='));
    }
}
