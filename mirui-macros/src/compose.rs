use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Result, Token, Visibility, braced};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<ComposeInput>(input) {
        Ok(parsed) => parsed.emit(),
        Err(e) => e.to_compile_error(),
    }
}

struct ComposeInput {
    vis: Visibility,
    name: Ident,
    fields: Vec<FieldDecl>,
    routes: Vec<Route>,
}

struct FieldDecl {
    name: Ident,
    // Parsed but unused in S1; S2 will consult it for method signatures.
    #[allow(dead_code)]
    ty: syn::Type,
}

struct Route {
    /// `default` or a method name from DrawBackend.
    method: Ident,
    field: Ident,
}

impl Parse for ComposeInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let vis: Visibility = input.parse()?;
        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;

        let struct_body;
        braced!(struct_body in input);
        let mut fields = Vec::new();
        while !struct_body.is_empty() {
            let field_name: Ident = struct_body.parse()?;
            struct_body.parse::<Token![:]>()?;
            let ty: syn::Type = struct_body.parse()?;
            fields.push(FieldDecl {
                name: field_name,
                ty,
            });
            if struct_body.peek(Token![,]) {
                struct_body.parse::<Token![,]>()?;
            }
        }

        let route_kw: Ident = input.parse()?;
        if route_kw != "route" {
            return Err(syn::Error::new(route_kw.span(), "expected `route` block"));
        }
        let route_body;
        braced!(route_body in input);
        let mut routes = Vec::new();
        while !route_body.is_empty() {
            let method: Ident = route_body.parse()?;
            route_body.parse::<Token![=>]>()?;
            let field: Ident = route_body.parse()?;
            routes.push(Route { method, field });
            if route_body.peek(Token![,]) {
                route_body.parse::<Token![,]>()?;
            }
        }

        Ok(ComposeInput {
            vis,
            name,
            fields,
            routes,
        })
    }
}

impl ComposeInput {
    fn emit(&self) -> TokenStream {
        if let Err(e) = self.validate() {
            return e.to_compile_error();
        }

        // __B prefix to avoid colliding with user type names.
        let generic_params: Vec<Ident> = (0..self.fields.len())
            .map(|i| format_ident!("__B{i}"))
            .collect();

        let struct_fields = self.fields.iter().zip(&generic_params).map(|(f, g)| {
            let name = &f.name;
            quote! { pub #name: #g }
        });

        let vis = &self.vis;
        let name = &self.name;

        quote! {
            #vis struct #name<#(#generic_params),*> {
                #(#struct_fields,)*
            }
        }
    }

    fn validate(&self) -> Result<()> {
        if self.fields.is_empty() {
            return Err(syn::Error::new(
                self.name.span(),
                "compose_backend! struct must have at least one field",
            ));
        }
        for (i, f) in self.fields.iter().enumerate() {
            for g in &self.fields[i + 1..] {
                if f.name == g.name {
                    return Err(syn::Error::new(
                        g.name.span(),
                        format!("field `{}` declared more than once", g.name),
                    ));
                }
            }
        }
        for r in &self.routes {
            if !self.fields.iter().any(|f| f.name == r.field) {
                return Err(syn::Error::new(
                    r.field.span(),
                    format!("field `{}` not declared in struct body", r.field),
                ));
            }
        }
        for (i, r) in self.routes.iter().enumerate() {
            for s in &self.routes[i + 1..] {
                if r.method == s.method {
                    return Err(syn::Error::new(
                        s.method.span(),
                        format!(
                            "method `{}` routed more than once (also at earlier position)",
                            s.method
                        ),
                    ));
                }
            }
        }
        if !self.routes.iter().any(|r| r.method == "default") {
            return Err(syn::Error::new(
                self.name.span(),
                "compose_backend! requires a `default => <field>` route",
            ));
        }
        Ok(())
    }
}
