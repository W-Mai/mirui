use super::*;

impl MiruiRune {
    pub(super) fn emit_match(
        cmd: &MatchCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            let read = reactive_read(&cmd.scrutinee);
            Self::emit_match_reactive(cmd, &read, world, parent_var)
        } else {
            Self::emit_match_static(cmd, world, parent_var)
        }
    }

    fn emit_match_reactive(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd
            .arms
            .iter()
            .any(|arm| Self::has_uncontained_niche(&arm.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "slot operations directly inside a reactive `match` arm need a widget parent",
            )
            .to_compile_error();
        }
        if cmd
            .arms
            .iter()
            .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "reactive control flow directly inside a reactive `match` arm needs a widget parent",
            )
            .to_compile_error();
        }
        if cmd.arms.iter().any(|arm| Self::pattern_binds(&arm.pat)) {
            return Self::emit_match_reactive_dynamic(cmd, read, world, parent_var);
        }
        Self::emit_match_reactive_cached(cmd, read, world, parent_var)
    }

    // Unknown pattern forms take the dynamic path, so an arm body can always
    // use any values its pattern introduces.
    pub(super) fn pattern_binds(pat: &syn::Pat) -> bool {
        match pat {
            syn::Pat::Const(_)
            | syn::Pat::Lit(_)
            | syn::Pat::Path(_)
            | syn::Pat::Range(_)
            | syn::Pat::Rest(_)
            | syn::Pat::Wild(_) => false,
            syn::Pat::Or(pat) => pat.cases.iter().any(Self::pattern_binds),
            syn::Pat::Paren(pat) => Self::pattern_binds(&pat.pat),
            syn::Pat::Reference(pat) => Self::pattern_binds(&pat.pat),
            syn::Pat::Slice(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::Struct(pat) => pat
                .fields
                .iter()
                .any(|field| Self::pattern_binds(&field.pat)),
            syn::Pat::Tuple(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::TupleStruct(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::Type(pat) => Self::pattern_binds(&pat.pat),
            // syn represents the prelude's bare `None` variant as an identifier.
            syn::Pat::Ident(pat)
                if pat.ident == "None"
                    && pat.by_ref.is_none()
                    && pat.mutability.is_none()
                    && pat.subpat.is_none() =>
            {
                false
            }
            syn::Pat::Ident(_) | syn::Pat::Macro(_) | syn::Pat::Verbatim(_) => true,
            _ => true,
        }
    }

    fn emit_match_reactive_cached(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let build_branches = cmd.arms.iter().map(|arm| {
            let body = Self::emit_branch_body_inline(&arm.body, world, &quote! { __parent_e });
            quote! {
                {
                    let __start = (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or(0, |children| children.0.len());
                    #body
                    (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or_else(mirui::__macro_support::__Vec::new, |children| children.0[__start..].to_vec())
                }
            }
        });
        let select_arms = cmd.arms.iter().enumerate().map(|(index, arm)| {
            let pat = &arm.pat;
            quote! { #pat => #index, }
        });

        quote! {
            {
                let __parent_e = #parent_var;
                let __branches = [#(#build_branches),*];
                mirui::ui::branch::prepare(#world, __parent_e, &__branches);
                let __selected = mirui::__macro_support::__Cell::new(None::<usize>);
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::__macro_support::effect_with_role(__parent_e, mirui::__macro_support::BindingRole::Maintenance, move || {
                        let __next = Some(match #read {
                            #(#select_arms)*
                        });
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::__macro_support::with_world(|__w| {
                                mirui::ui::branch::select(__w, &__branches, __previous, __next);
                                __selected.set(__next);
                            });
                        }
                    });
                });
            }
        }
    }

    // A binding is only in scope while its selected arm is built. Bound arms
    // keep structural rebuild behavior, with all top-level roots managed.
    fn emit_match_reactive_dynamic(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let select_arms = cmd.arms.iter().map(|arm| {
            let pat = &arm.pat;
            let body =
                Self::emit_branch_body_inline(&arm.body, &quote! { __w }, &quote! { __parent_e });
            quote! { #pat => { #body }, }
        });
        quote! {
            {
                let __mounted = mirui::__macro_support::__RefCell::new(mirui::__macro_support::__Vec::<mirui::ecs::Entity>::new());
                let __parent_e = #parent_var;
                let __mount_index = (#world)
                    .get::<mirui::ui::Children>(__parent_e)
                    .map_or(0, |children| children.0.len());
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::__macro_support::effect_with_role(__parent_e, mirui::__macro_support::BindingRole::Maintenance, move || {
                        let __sel = #read;
                        mirui::__macro_support::with_world(|__w| {
                            let mut __mounted = __mounted.borrow_mut();
                            let __idx = __mounted.first().and_then(|o| {
                                __w.get::<mirui::ui::Children>(__parent_e)
                                    .and_then(|c| c.0.iter().position(|e| e == o))
                            }).unwrap_or(__mount_index);
                            for __o in __mounted.drain(..) {
                                mirui::__macro_support::despawn_subtree(__w, __o);
                            }
                            let __start = __w
                                .get::<mirui::ui::Children>(__parent_e)
                                .map_or(0, |children| children.0.len());
                            match __sel {
                                #(#select_arms)*
                            }
                            if let Some(children) = __w.get_mut::<mirui::ui::Children>(__parent_e) {
                                __mounted.extend(children.0.drain(__start..));
                                let __pos = __idx.min(children.0.len());
                                for (offset, &root) in __mounted.iter().enumerate() {
                                    children.0.insert(__pos + offset, root);
                                }
                            }
                        });
                    });
                });
            }
        }
    }

    fn emit_match_static(
        cmd: &MatchCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let scrutinee = &cmd.scrutinee;
        let arm_tokens = cmd.arms.iter().map(|arm| {
            let pat = &arm.pat;
            let body_tokens: proc_macro2::TokenStream =
                Self::emit_siblings(&arm.body, world, parent_var).collect();
            quote! { #pat => { #body_tokens } }
        });

        quote! {
            match #scrutinee {
                #(#arm_tokens)*
            }
        }
    }

    pub(super) fn emit_niche(
        cmd: &NicheCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.is_declaration {
            if let Some(slot_var) = &cmd.mold_slot_var {
                let slot_name = cmd.name.to_string();
                let slot_var_ts = quote! { #slot_var };
                let fallback: proc_macro2::TokenStream =
                    Self::emit_siblings(&cmd.body, world, &slot_var_ts).collect();
                return quote! {
                    let __slot_parent = #parent_var;
                    let #slot_var = ::mirui::__macro_support::WidgetBuilder::new(#world).id();
                    ::mirui::__macro_support::finish_widget_static(#world, __slot_parent, #slot_var);
                    __mold_niche_map.insert(#slot_name, #slot_var);
                    #fallback
                };
            }
            let msg = format!(
                "ui!: `@@{}` is a slot declaration; use it inside `ui!(compose ...)` declarations — call sites fill slots with the single-`@` form `@{}` {{ ... }}",
                cmd.name, cmd.name,
            );
            return quote! {
                compile_error!(#msg);
            };
        }

        let niche_name = cmd.name.to_string();
        let niche_var = syn::Ident::new(
            &format!("__niche_{}", cmd.name),
            proc_macro2::Span::call_site(),
        );
        let niche_var_ts = quote! { #niche_var };

        let slot_check = if let Some(pw) = &cmd.parent_widget {
            let slot_method = syn::Ident::new(&format!("__slot_{}", cmd.name), cmd.name.span());
            quote! {
                let _ = <#pw>::#slot_method;
            }
        } else {
            quote! {}
        };

        let body_tokens: proc_macro2::TokenStream =
            Self::emit_siblings(&cmd.body, world, &niche_var_ts).collect();

        let widget_label = quote! { stringify!(#parent_var) };
        quote! {
            #slot_check
            let #niche_var = match (#world).get::<mirui::ui::NicheMap>(#parent_var) {
                Some(map) => match map.get(#niche_name) {
                    Some(e) => e,
                    None => panic!(
                        "ui!: niche '{}' not registered on widget {}",
                        #niche_name,
                        #widget_label,
                    ),
                },
                None => panic!(
                    "ui!: widget {} has no NicheMap (missing auto_attach?)",
                    #widget_label
                ),
            };
            {
                let __existing: ::mirui::__macro_support::__Vec<::mirui::ecs::Entity> = (#world)
                    .get::<::mirui::ui::Children>(#niche_var_ts)
                    .map(|c| c.0.clone())
                    .unwrap_or_default();
                for __e in __existing {
                    ::mirui::__macro_support::despawn_subtree(#world, __e);
                }
            }
            #body_tokens
        }
    }

    pub(super) fn emit_iter(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            Self::emit_iter_reactive(cmd, world, parent_var)
        } else {
            Self::emit_iter_static(cmd, world, parent_var)
        }
    }

    fn emit_iter_static(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;

        let body_tokens: proc_macro2::TokenStream =
            Self::emit_siblings(&cmd.body, world, parent_var).collect();

        quote! {
            for #variable in #iterable {
                #body_tokens
            }
        }
    }

    pub(super) fn emit_if(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            Self::emit_if_reactive(cmd, world, parent_var)
        } else {
            Self::emit_if_static(cmd, world, parent_var)
        }
    }

    fn emit_branch_body_inline(
        body: &[Cmd],
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        Self::emit_siblings(body, world, parent_var).collect()
    }

    fn emit_if_static(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let mut chain = proc_macro2::TokenStream::new();
        for (i, br) in cmd.branches.iter().enumerate() {
            let body = Self::emit_branch_body_inline(&br.body, world, parent_var);
            match (&br.cond, i) {
                (Some(c), 0) => chain.extend(quote! { if #c { #body } }),
                (Some(c), _) => chain.extend(quote! { else if #c { #body } }),
                (None, _) => chain.extend(quote! { else { #body } }),
            }
        }
        chain
    }

    pub(super) fn collect_children(&mut self, children: &[DsTreeRef]) -> Vec<Cmd> {
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        self.stack.pop().unwrap()
    }

    fn walk_uses_forest(body: &[Cmd]) -> bool {
        !matches!(body, [Cmd::Widget(_) | Cmd::Compose(_)])
    }

    fn has_replacing_walk_roots(body: &[Cmd]) -> bool {
        body.iter().any(|cmd| match cmd {
            Cmd::Iter(iter) => iter.reactive || Self::has_replacing_walk_roots(&iter.body),
            Cmd::Match(branch) => {
                branch.reactive && branch.arms.iter().any(|arm| Self::pattern_binds(&arm.pat))
                    || branch
                        .arms
                        .iter()
                        .any(|arm| Self::has_replacing_walk_roots(&arm.body))
            }
            Cmd::If(branch) => branch
                .branches
                .iter()
                .any(|arm| Self::has_replacing_walk_roots(&arm.body)),
            Cmd::Niche(niche) => Self::has_replacing_walk_roots(&niche.body),
            Cmd::Widget(_) | Cmd::Compose(_) | Cmd::CodeBlock(_) => false,
        })
    }

    // Single rooted members retain their direct path; compound members own
    // every top-level root and the effects created during their construction.
    fn branch_body(body: &[Cmd]) -> proc_macro2::TokenStream {
        let w = quote! { __w };
        let p = quote! { __parent };
        if Self::walk_uses_forest(body) {
            if Self::has_replacing_walk_roots(body) {
                return syn::Error::new(proc_macro2::Span::call_site(),
                    "reactive iteration and payload-binding matches directly inside a walk body need a widget parent")
                    .to_compile_error();
            }
            let stmts: proc_macro2::TokenStream = Self::emit_siblings(body, &w, &p).collect();
            return quote! {
                Some(mirui::__macro_support::with_owned_effects(|| {
                    let __start = (#w).get::<mirui::ui::Children>(#p)
                        .map_or(0, |children| children.0.len());
                    #stmts
                    (#w).get_mut::<mirui::ui::Children>(#p)
                        .map_or_else(mirui::__macro_support::__Vec::new,
                            |children| children.0.split_off(__start))
                }))
            };
        }
        let mut stmts = proc_macro2::TokenStream::new();
        let mut root: Option<proc_macro2::TokenStream> = None;
        for (child, statement) in body.iter().zip(Self::emit_siblings(body, &w, &p)) {
            stmts.extend(statement);
            let child_var = match child {
                Cmd::Widget(cw) => Some(&cw.var),
                Cmd::Compose(compose) => Some(&compose.var),
                _ => None,
            };
            if let Some(cv) = child_var
                && root.is_none()
            {
                stmts.extend(quote! {
                    if let Some(children) = (#w).get_mut::<mirui::ui::Children>(#p) {
                        if let Some(index) = children.0.iter().position(|entity| *entity == #cv) {
                            children.0.remove(index);
                        }
                    }
                });
                root = Some(quote! { #cv });
            }
        }
        let ret = match root {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };
        quote! { #stmts #ret }
    }

    fn has_uncontained_reactive_child(body: &[Cmd]) -> bool {
        body.iter().any(|child| match child {
            Cmd::If(branch) => {
                branch.reactive
                    || branch
                        .branches
                        .iter()
                        .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
            }
            Cmd::Match(branch) => {
                branch.reactive
                    || branch
                        .arms
                        .iter()
                        .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
            }
            Cmd::Iter(iter) => iter.reactive || Self::has_uncontained_reactive_child(&iter.body),
            Cmd::Niche(niche) => Self::has_uncontained_reactive_child(&niche.body),
            Cmd::Widget(_) | Cmd::Compose(_) | Cmd::CodeBlock(_) => false,
        })
    }

    fn has_uncontained_niche(body: &[Cmd]) -> bool {
        body.iter().any(|child| match child {
            Cmd::Niche(_) => true,
            Cmd::If(branch) => branch
                .branches
                .iter()
                .any(|arm| Self::has_uncontained_niche(&arm.body)),
            Cmd::Match(branch) => branch
                .arms
                .iter()
                .any(|arm| Self::has_uncontained_niche(&arm.body)),
            Cmd::Iter(iter) => Self::has_uncontained_niche(&iter.body),
            Cmd::Widget(_) | Cmd::Compose(_) | Cmd::CodeBlock(_) => false,
        })
    }

    fn emit_if_reactive(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd
            .branches
            .iter()
            .any(|branch| Self::has_uncontained_niche(&branch.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "slot operations directly inside a cached `if` branch need a widget parent",
            )
            .to_compile_error();
        }
        if cmd
            .branches
            .iter()
            .any(|branch| Self::has_uncontained_reactive_child(&branch.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "reactive control flow directly inside a cached `if` branch needs a widget parent",
            )
            .to_compile_error();
        }
        let build_branches = cmd.branches.iter().map(|branch| {
            let body = Self::emit_branch_body_inline(&branch.body, world, &quote! { __parent_e });
            quote! {
                {
                    let __start = (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or(0, |children| children.0.len());
                    #body
                    (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or_else(mirui::__macro_support::__Vec::new, |children| children.0[__start..].to_vec())
                }
            }
        });

        let mut cascade = proc_macro2::TokenStream::new();
        let mut first = true;
        let mut has_else = false;
        for (index, br) in cmd.branches.iter().enumerate() {
            match &br.cond {
                Some(cond) => {
                    let read = reactive_read(cond);
                    let head = if first { quote!(if) } else { quote!(else if) };
                    cascade.extend(quote! { #head (#read) { Some(#index) } });
                    first = false;
                }
                None => {
                    cascade.extend(quote! { else { Some(#index) } });
                    has_else = true;
                }
            }
        }
        if !has_else {
            cascade.extend(quote! { else { None } });
        }

        quote! {
            {
                let __parent_e = #parent_var;
                let __branches = [#(#build_branches),*];
                mirui::ui::branch::prepare(#world, __parent_e, &__branches);
                let __selected = mirui::__macro_support::__Cell::new(None::<usize>);
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::__macro_support::effect_with_role(__parent_e, mirui::__macro_support::BindingRole::Maintenance, move || {
                        let __next: ::core::option::Option<usize> = #cascade;
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::__macro_support::with_world(|__w| {
                                mirui::ui::branch::select(__w, &__branches, __previous, __next);
                                __selected.set(__next);
                            });
                        }
                    });
                });
            }
        }
    }

    fn emit_iter_reactive(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.key.is_some() {
            Self::emit_iter_reactive_keyed(cmd, world, parent_var)
        } else {
            Self::emit_iter_reactive_indexed(cmd, world, parent_var)
        }
    }

    fn emit_iter_reactive_indexed(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;
        let row_body = Self::branch_body(&cmd.body);
        let forest = Self::walk_uses_forest(&cmd.body);
        let row_type = if forest {
            quote! { (mirui::__macro_support::__Vec<mirui::ecs::Entity>, mirui::__macro_support::OwnedEffects) }
        } else {
            quote! { mirui::ecs::Entity }
        };
        let attach = if forest {
            quote! { children.0.extend_from_slice(&__r.0); }
        } else {
            quote! { children.0.push(__r); }
        };
        let remove_root = quote! {
            let __pos = __w.get::<mirui::ui::Children>(__parent_e)
                .and_then(|children| children.0.iter().position(|entity| *entity == __e));
            if let Some(__pos) = __pos {
                if let Some(children) = __w.get_mut::<mirui::ui::Children>(__parent_e) {
                    children.0.remove(__pos);
                }
            }
            mirui::__macro_support::despawn_subtree(__w, __e);
        };
        let retire = if forest {
            quote! { for __e in __row.0 { #remove_root } }
        } else {
            quote! { let __e = __row; #remove_root }
        };

        quote! {
            {
                let __rows = mirui::__macro_support::__RefCell::new(
                    mirui::__macro_support::__Vec::<#row_type>::new(),
                );
                let __parent_e = #parent_var;
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::__macro_support::effect_with_role(__parent_e, mirui::__macro_support::BindingRole::Maintenance, move || {
                        let __items = (#iterable).into_iter();
                        mirui::__macro_support::with_world(|__w| {
                            let __n_old = __rows.borrow().len();
                            let mut __n_new = 0;
                            for #variable in __items {
                                __n_new += 1;
                                if __n_new <= __n_old {
                                    continue;
                                }
                                let __r = (|__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity| -> Option<#row_type> {
                                    #row_body
                                })(__w, __parent_e);
                                if let Some(__r) = __r {
                                    if let Some(children) =
                                        __w.get_mut::<mirui::ui::Children>(__parent_e)
                                    {
                                        #attach
                                    }
                                    __rows.borrow_mut().push(__r);
                                }
                            }
                            if __n_new < __n_old {
                                for __row in __rows.borrow_mut().drain(__n_new..) {
                                    #retire
                                }
                            }
                        });
                    });
                });
            }
        }
    }

    fn emit_iter_reactive_keyed(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;
        let key_expr = cmd.key.as_ref().unwrap();
        let row_body = Self::branch_body(&cmd.body);
        let forest = Self::walk_uses_forest(&cmd.body);
        let row_type = if forest {
            quote! { (mirui::__macro_support::__Vec<mirui::ecs::Entity>, mirui::__macro_support::OwnedEffects) }
        } else {
            quote! { mirui::ecs::Entity }
        };
        let roots = if forest {
            quote! { __new.iter().flat_map(|(_, row)| row.0.iter().copied()) }
        } else {
            quote! { __new.iter().map(|(_, entity)| *entity) }
        };
        let count = if forest {
            quote! { __new.iter().map(|(_, row)| row.0.len()).sum::<usize>() }
        } else {
            quote! { __new.len() }
        };
        let contains = if forest {
            quote! { __new.iter().any(|(_, row)| row.0.contains(entity)) }
        } else {
            quote! { __new.iter().any(|(_, row)| row == entity) }
        };
        let retire = if forest {
            quote! {
                for __e in __row.0 {
                    mirui::__macro_support::despawn_subtree(__w, __e);
                }
            }
        } else {
            quote! { mirui::__macro_support::despawn_subtree(__w, __row); }
        };

        quote! {
            {
                let __rows = mirui::__macro_support::__RefCell::new((
                    mirui::__macro_support::__Vec::new(),
                    mirui::__macro_support::__Vec::new(),
                ));
                let __parent_e = #parent_var;
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::__macro_support::effect_with_role(__parent_e, mirui::__macro_support::BindingRole::Maintenance, move || {
                        let __items = (#iterable).into_iter();
                        mirui::__macro_support::with_world(|__w| {
                            let mut __buffers = __rows.borrow_mut();
                            let (__old, __new) = &mut *__buffers;
                            __new.clear();
                            for #variable in __items {
                                let __k = #key_expr;
                                if let Some(__i) = __old.iter().position(|(__ok, _)| *__ok == __k) {
                                    let (_, __e) = __old.remove(__i);
                                    __new.push((__k, __e));
                                } else {
                                    let __r = (|__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity| -> Option<#row_type> {
                                        #row_body
                                    })(__w, __parent_e);
                                    if let Some(__r) = __r {
                                        __new.push((__k, __r));
                                    }
                                }
                            }
                            for (_, __row) in __old.drain(..) {
                                #retire
                            }
                            // Retain unrelated siblings, then append the managed rows.
                            // An already ordered row suffix needs no mutable borrow.
                            let __children_changed = __w.get::<mirui::ui::Children>(__parent_e)
                                .is_some_and(|children| {
                                    let Some(prefix_len) = children.0.len().checked_sub(#count) else {
                                        return true;
                                    };
                                    let (siblings, rows) = children.0.split_at(prefix_len);
                                    !rows.iter().copied().eq(#roots)
                                        || siblings.iter().any(|entity| #contains)
                                });
                            if __children_changed
                                && let Some(children) = __w.get_mut::<mirui::ui::Children>(__parent_e)
                            {
                                children.0.retain(|entity| !#contains);
                                children.0.extend(#roots);
                            }
                            __old.reserve(__new.len());
                            ::core::mem::swap(__old, __new);
                        });
                    });
                });
            }
        }
    }
}
