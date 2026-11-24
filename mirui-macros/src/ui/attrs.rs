use super::*;

impl MiruiRune {
    pub(crate) fn parse_attrs(
        &self,
        attrs: &[DsAttr],
        widget_name: &str,
        widget_kind: WidgetKind,
    ) -> ParsedAttrs {
        let mut builder_calls = Vec::new();
        let mut layout_fields = Vec::new();
        let mut preset = None;
        let mut classes = None;
        let mut style_fields = std::collections::BTreeSet::new();
        let mut errors = Vec::new();
        let mut component_inserts = Vec::new();
        let mut component_fields = Vec::new();
        let mut field_bindings = Vec::new();
        let mut component_field_names = std::collections::BTreeSet::new();
        let mut text_tuple_value: Option<proc_macro2::TokenStream> = None;
        let mut capacity: Option<proc_macro2::TokenStream> = None;
        let mut preedit_capacity: Option<proc_macro2::TokenStream> = None;
        let mut text_paragraph_value: Option<proc_macro2::TokenStream> = None;
        let mut text_line_height: Option<proc_macro2::TokenStream> = None;
        let mut id = None;
        let mut id_lookups: Vec<(syn::Ident, String)> = Vec::new();
        let mut reactive_binds: Vec<ReactiveBind> = Vec::new();
        let mut property_values = Vec::new();
        let mut layout_binds = Vec::new();
        let mut user_set_direction = false;

        let is_text_widget = widget_name == "Text";
        let is_button_widget = widget_name == "Button";
        let is_text_input_widget = widget_name == "TextInput";
        const TEXT_INPUT_FIELDS: &[&str] = &[
            "placeholder_color",
            "cursor_color",
            "focus_border_color",
            "lines",
        ];

        let mut positional_consumed = false;
        for attr in attrs {
            let mut rewritten = attr.value.clone();
            syn::visit_mut::VisitMut::visit_expr_mut(
                &mut crate::visit_id::IdRewriter {
                    captured: &mut id_lookups,
                },
                &mut rewritten,
            );
            let value = &rewritten;
            let normalized;
            let value = if attr.reactive {
                normalized = compose_input::normalize(value, &self.props);
                &normalized
            } else {
                value
            };

            let name = match &attr.name {
                Some(n) => n.to_string(),
                None => match (primary_attr_for(widget_name), positional_consumed) {
                    (Some(primary), false) => {
                        positional_consumed = true;
                        primary.to_string()
                    }
                    (Some(_), true) => {
                        errors.push(
                            syn::Error::new(
                                syn::spanned::Spanned::span(&attr.value),
                                format!(
                                    "{widget_name} accepts only one positional argument; pass extra fields by name",
                                ),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                    (None, _) => {
                        errors.push(
                            syn::Error::new(
                                syn::spanned::Spanned::span(&attr.value),
                                format!(
                                    "{widget_name} does not accept positional arguments; use `name: value` form",
                                ),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                },
            };

            let attr_span = attr
                .name
                .as_ref()
                .map(|n| n.span())
                .unwrap_or_else(|| syn::spanned::Spanned::span(&attr.value));

            if matches!(name.as_str(), "style" | "class") {
                let target = if name == "style" {
                    &mut preset
                } else {
                    &mut classes
                };
                let next = StyleAttr {
                    value: value.clone(),
                    reactive: attr.reactive,
                    static_input: compose_input::direct_prop(&attr.value, &self.props).cloned(),
                };
                if target.replace(next).is_some() {
                    errors.push(
                        syn::Error::new(attr_span, format!("duplicate `{name}` attribute"))
                            .to_compile_error(),
                    );
                }
                continue;
            }
            style_fields.extend(style::fields(&name).map(|field| field.key));

            if name == "preedit_capacity" && is_text_input_widget {
                if preedit_capacity.is_some() {
                    errors.push(
                        syn::Error::new(attr_span, "duplicate `preedit_capacity`")
                            .to_compile_error(),
                    );
                } else if attr.reactive {
                    errors.push(
                        syn::Error::new(
                            attr_span,
                            "`preedit_capacity` must be a compile-time constant",
                        )
                        .to_compile_error(),
                    );
                } else {
                    preedit_capacity = Some(quote! { #value });
                }
                continue;
            }
            if name == "capacity" && (is_text_widget || is_button_widget || is_text_input_widget) {
                if capacity.is_some() {
                    errors.push(
                        syn::Error::new(attr_span, "duplicate `capacity`").to_compile_error(),
                    );
                } else if attr.reactive {
                    errors.push(
                        syn::Error::new(attr_span, "`capacity` must be a compile-time constant")
                            .to_compile_error(),
                    );
                } else {
                    capacity = Some(quote! { #value });
                }
                continue;
            }

            if crate::layout_reactive::is_layout_placeholder(&attr.value) {
                match responsive_property(widget_name, &name) {
                    Some(property) => {
                        match crate::layout_reactive::LayoutValue::parse_expr(&attr.value) {
                            Ok(Some(value)) => layout_binds.push(LayoutBind {
                                property: syn::Ident::new(property, attr_span),
                                value,
                            }),
                            Ok(None) => unreachable!("layout placeholder already checked"),
                            Err(error) => errors.push(error.to_compile_error()),
                        }
                    }
                    None => errors.push(
                        syn::Error::new(
                            attr_span,
                            format!("`{name}` does not support layout-responsive `@` binding"),
                        )
                        .to_compile_error(),
                    ),
                }
                continue;
            }

            if name == "container" {
                match value {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Bool(enabled),
                        ..
                    }) if enabled.value => {
                        component_inserts.push(quote! { ::mirui::ui::LayoutContainer });
                    }
                    _ => errors.push(
                        syn::Error::new_spanned(value, "`container` only accepts `true`")
                            .to_compile_error(),
                    ),
                }
                continue;
            }

            if name == "scroll_offset" && !attr.reactive {
                let property = syn::Ident::new(
                    reactive_property(widget_name, &name).expect("scroll offset property"),
                    attr_span,
                );
                property_values.push((property, quote! { #value }));
                continue;
            }

            // Must run before the Text / Component `text` routes below, or a
            // reactive `text` gets frozen as static content instead of bound.
            if attr.reactive {
                if widget_kind == WidgetKind::Component
                    && ((name == "text" && !is_text_widget && !is_button_widget)
                        || reactive_property(widget_name, &name).is_none())
                {
                    let field = syn::Ident::new(&name, attr_span);
                    if !component_field_names.insert(name.clone()) {
                        errors.push(
                            syn::Error::new(
                                attr_span,
                                format!("duplicate component property `{field}`"),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                    let marker = quote::format_ident!("__mirui_property_{field}");
                    let install = quote::format_ident!(
                        "__mirui_install_{field}",
                        span = proc_macro2::Span::mixed_site()
                    );
                    let value_name =
                        syn::Ident::new("__mirui_value", proc_macro2::Span::mixed_site());
                    let read = reactive_read(value);
                    let once = compose_input::direct_prop(&attr.value, &self.props)
                        .map_or_else(|| quote!(false), |input| quote!(#input.__mirui_is_static()));
                    component_fields.push(quote! {
                        let #install = {
                            // An inherent field declaration selects a typed property. Components
                            // without one keep the existing forwarded Prop conversion.
                            #[allow(dead_code)]
                            trait ForwardInput {
                                fn #marker(&self) -> ::mirui::__macro_support::ForwardProp {
                                    ::mirui::__macro_support::ForwardProp
                                }
                            }
                            impl<T> ForwardInput for T {}
                            __c.#marker().__mirui_prepare_field(&mut __c.#field, #once, move || {
                                let #value_name = #read;
                                ::core::convert::Into::into(#value_name)
                            })
                        };
                    });
                    field_bindings.push(install);
                    continue;
                }
                match reactive_property(widget_name, &name) {
                    Some(property) => {
                        reactive_binds.push(ReactiveBind {
                            property: syn::Ident::new(property, attr_span),
                            expr: reactive_read(value),
                            format_expr: value.clone(),
                            static_input: compose_input::direct_prop(&attr.value, &self.props)
                                .cloned(),
                        });
                        continue;
                    }
                    None => {
                        errors.push(
                            syn::Error::new(
                                attr_span,
                                format!("`{name}` does not support reactive `$` binding"),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                }
            }

            if (is_text_widget || is_button_widget) && name == "text" {
                text_tuple_value = Some(quote! { #value });
                continue;
            }

            if (is_text_widget || is_button_widget) && name == "paragraph" {
                text_paragraph_value = Some(quote! { #value });
                continue;
            }

            if (is_text_widget || is_button_widget) && name == "line_height" {
                text_line_height = Some(quote! { #value });
                continue;
            }

            if is_text_widget && name == "path" {
                builder_calls.push(quote! { .text_path(#value) });
                continue;
            }

            if is_text_input_widget && TEXT_INPUT_FIELDS.contains(&name.as_str()) {
                let method = syn::Ident::new(&format!("with_{name}"), attr_span);
                component_fields.push(quote! { .#method(#value) });
                continue;
            }

            if widget_kind == WidgetKind::Component && name == "text" {
                let field_ident = syn::Ident::new(&name, attr_span);
                let shared = compose_input::share(value, &self.props);
                component_fields.push(Self::field_init_tokens(
                    &field_ident,
                    &shared,
                    &mut component_field_names,
                ));
                continue;
            }

            let mut is_layout = false;
            for field in style::fields(&name).filter(|field| field.target == style::Target::Layout)
            {
                let key = quote::format_ident!("{}", field.key);
                let value = field.layout_value(value, false);
                layout_fields.push(quote! { #key: #value });
                is_layout = true;
            }
            if is_layout {
                user_set_direction |= name == "direction";
                continue;
            }

            if style::fields(&name).any(|field| field.target == style::Target::Common) {
                if name == "border_color" {
                    builder_calls.push(quote! { .border(#value, 1) });
                } else {
                    let name = quote::format_ident!("{name}");
                    builder_calls.push(quote! { .#name(#value) });
                }
                continue;
            }

            match name.as_str() {
                "text" => builder_calls.push(quote! { .text(#value) }),
                "image" => builder_calls.push(quote! { .image(#value) }),
                "id" => match Self::extract_id_str(value) {
                    Some(s) => id = Some(s),
                    None => errors.push(
                        syn::Error::new_spanned(
                            value,
                            "id attribute must be a string literal, e.g. `id: \"submit\"`",
                        )
                        .to_compile_error(),
                    ),
                },
                unknown => match widget_kind {
                    WidgetKind::Component => {
                        let field_ident = syn::Ident::new(unknown, attr_span);
                        let shared = compose_input::share(value, &self.props);
                        component_fields.push(Self::field_init_tokens(
                            &field_ident,
                            &shared,
                            &mut component_field_names,
                        ));
                    }
                    WidgetKind::Layout | WidgetKind::IllegalLowercase => {
                        let mut msg = format!("unknown widget attribute `{unknown}`");
                        let candidates =
                            style::common_names().chain(["text", "image", "id", "style", "class"]);
                        if let Some(hint) = crate::diag::closest(unknown, candidates, 2) {
                            msg.push_str(&format!(". did you mean `{hint}`?"));
                        }
                        errors.push(syn::Error::new(attr_span, msg).to_compile_error());
                    }
                },
            }
        }
        if let Some(line_height) = text_line_height {
            let paragraph = text_paragraph_value.unwrap_or_else(|| {
                if is_button_widget {
                    quote! { ::mirui::ui::widgets::ParagraphStyle::label() }
                } else {
                    quote! { ::mirui::ui::widgets::ParagraphStyle::default() }
                }
            });
            text_paragraph_value = Some(quote! { (#paragraph).with_line_height(#line_height) });
        }
        if is_text_widget {
            // Text is built from its content and paragraph alone, so any other
            // field would be dropped without a word.
            for field in &component_field_names {
                errors.push(
                    syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!("Text has no attribute `{field}`"),
                    )
                    .to_compile_error(),
                );
            }
        }
        ParsedAttrs {
            builder_calls,
            layout_fields,
            preset,
            classes,
            style_fields,
            errors,
            component_inserts,
            component_fields,
            field_bindings,
            text_tuple_value,
            capacity,
            preedit_capacity,
            text_paragraph_value,
            id,
            id_lookups,
            reactive_binds,
            property_values,
            layout_binds,
            user_set_direction,
        }
    }

    fn extract_id_str(expr: &syn::Expr) -> Option<syn::LitStr> {
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = expr
        {
            Some(s.clone())
        } else {
            None
        }
    }

    fn field_init_tokens(
        field_ident: &syn::Ident,
        value: &syn::Expr,
        names: &mut std::collections::BTreeSet<String>,
    ) -> proc_macro2::TokenStream {
        if !names.insert(field_ident.to_string()) {
            return syn::Error::new(
                field_ident.span(),
                format!("duplicate component property `{field_ident}`"),
            )
            .to_compile_error();
        }
        let is_float_literal = matches!(
            value,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(_),
                ..
            })
        );
        if is_float_literal {
            quote! { __c.#field_ident = #value; }
        } else {
            quote! { __c.#field_ident = (#value).into(); }
        }
    }
}
