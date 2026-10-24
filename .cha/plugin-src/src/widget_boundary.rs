use std::collections::BTreeMap;

use proc_macro2::Span;
use syn::{ext::IdentExt, spanned::Spanned, visit::Visit};

use crate::{AnalysisInput, FileRole, Finding, OptionValue, Severity, SmellCategory, finding};

const SMELL: &str = "widget-dependency-boundary";

pub(super) fn check(input: &AnalysisInput, findings: &mut Vec<Finding>) {
    if input.role == FileRole::Test {
        return;
    }
    let protected = cha_plugin_sdk::option_list_str!(&input.options, "widget_boundary_paths")
        .unwrap_or_default();
    let allowed = cha_plugin_sdk::option_list_str!(&input.options, "widget_boundary_symbols")
        .unwrap_or_default();
    findings.extend(analyze(&input.path, &input.content, protected, allowed));
}

fn source_path(path: &str) -> &str {
    path.rfind("/src/").map_or(path, |index| &path[index + 1..])
}

fn analyze(path: &str, source: &str, protected: &[String], allowed: &[String]) -> Vec<Finding> {
    let relative = source_path(path);
    if !protected
        .iter()
        .any(|prefix| relative == prefix || (prefix.ends_with('/') && relative.starts_with(prefix)))
    {
        return Vec::new();
    }
    let mut checker = Checker {
        path,
        relative,
        module: module_path(relative),
        scopes: Vec::new(),
        allowed,
        findings: Vec::new(),
    };
    match syn::parse_file(source) {
        Ok(file) => checker.visit_file(&file),
        Err(error) => checker.report(error.span(), "unparsed Rust source", true),
    }
    checker.findings
}

fn module_path(path: &str) -> Vec<String> {
    let mut parts: Vec<_> = path
        .strip_prefix("src/")
        .unwrap_or(path)
        .trim_end_matches(".rs")
        .split('/')
        .map(str::to_owned)
        .collect();
    if parts
        .last()
        .is_some_and(|part| part == "mod" || part == "lib")
    {
        parts.pop();
    }
    parts.insert(0, "crate".into());
    parts
}

struct Import {
    path: Vec<String>,
    alias: String,
    span: Span,
}

fn imports(tree: &syn::UseTree, prefix: &[String], result: &mut Vec<Import>) {
    match tree {
        syn::UseTree::Path(path) => {
            let mut prefix = prefix.to_vec();
            prefix.push(path.ident.unraw().to_string());
            imports(&path.tree, &prefix, result);
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                imports(item, prefix, result);
            }
        }
        leaf => {
            let (name, alias) = match leaf {
                syn::UseTree::Name(name) => (
                    name.ident.unraw().to_string(),
                    name.ident.unraw().to_string(),
                ),
                syn::UseTree::Rename(rename) => (
                    rename.ident.unraw().to_string(),
                    rename.rename.unraw().to_string(),
                ),
                syn::UseTree::Glob(_) => ("*".into(), "*".into()),
                _ => unreachable!(),
            };
            let mut path = prefix.to_vec();
            if name != "self" {
                path.push(name);
            }
            result.push(Import {
                path,
                alias,
                span: leaf.span(),
            });
        }
    }
}

fn test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

fn item_attributes(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

struct Checker<'a> {
    path: &'a str,
    relative: &'a str,
    module: Vec<String>,
    scopes: Vec<Scope>,
    allowed: &'a [String],
    findings: Vec<Finding>,
}

struct Scope {
    module: Option<Vec<String>>,
    bindings: BTreeMap<String, Vec<String>>,
}

impl Checker<'_> {
    fn enter<'a>(&mut self, items: impl Iterator<Item = &'a syn::Item>, module: bool) {
        let mut bindings = BTreeMap::new();
        for item in items {
            if let syn::Item::ExternCrate(item) = item
                && item.ident == "self"
                && !test_only(&item.attrs)
                && let Some((_, alias)) = &item.rename
            {
                bindings.insert(alias.unraw().to_string(), vec!["crate".into()]);
            }
            if let syn::Item::Use(item) = item {
                if test_only(&item.attrs) {
                    continue;
                }
                let mut leaves = Vec::new();
                imports(&item.tree, &[], &mut leaves);
                for leaf in leaves {
                    if leaf.alias != "*" {
                        bindings.insert(leaf.alias, self.relative_path(leaf.path));
                    }
                }
            }
        }
        self.scopes.push(Scope {
            module: module.then(|| self.module.clone()),
            bindings,
        });
    }

    fn resolve(&self, path: &[String]) -> Vec<String> {
        let mut path = self.relative_path(path.to_vec());
        let mut visited = Vec::new();
        let limit = self
            .scopes
            .iter()
            .map(|scope| scope.bindings.len())
            .sum::<usize>();
        for _ in 0..=limit {
            if visited.contains(&path) {
                break;
            }
            visited.push(path.clone());
            let replacement = self.scopes.iter().rev().find_map(|scope| {
                if let Some(module) = &scope.module
                    && path.starts_with(module)
                    && let Some(binding) = path
                        .get(module.len())
                        .and_then(|name| scope.bindings.get(name))
                {
                    return Some((module.len() + 1, binding));
                }
                path.first()
                    .and_then(|name| scope.bindings.get(name))
                    .map(|binding| (1, binding))
            });
            let Some((length, replacement)) = replacement else {
                break;
            };
            path.splice(..length, replacement.iter().cloned());
        }
        path
    }

    fn relative_path(&self, path: Vec<String>) -> Vec<String> {
        match path.first().map(String::as_str) {
            Some("crate") => path,
            Some("mirui") => {
                let mut path = path;
                path[0] = "crate".into();
                path
            }
            Some("self" | "super") => {
                let mut resolved = self.module.clone();
                let mut offset = 0;
                while let Some(part) = path.get(offset) {
                    match part.as_str() {
                        "self" => {}
                        "super" => {
                            resolved.pop();
                        }
                        _ => break,
                    }
                    offset += 1;
                }
                resolved.extend(path[offset..].iter().cloned());
                resolved
            }
            _ => path,
        }
    }

    fn inspect(&mut self, path: &[String], span: Span) {
        let canonical = self.resolve(path).join("::");
        if canonical == "crate::ui::widgets"
            || canonical.starts_with("crate::ui::widgets::")
            || canonical == "crate::prelude"
            || canonical.starts_with("crate::prelude::")
        {
            let allowed = self.allowed.iter().any(|entry| {
                entry
                    .split_once('|')
                    .is_some_and(|(file, symbol)| file == self.relative && symbol == canonical)
            });
            if !allowed {
                self.report(span, &canonical, false);
            }
        }
    }

    fn inspect_tokens(&mut self, stream: proc_macro2::TokenStream) {
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            match &tokens[index] {
                proc_macro2::TokenTree::Group(group) => {
                    self.inspect_tokens(group.stream());
                }
                proc_macro2::TokenTree::Ident(first) => {
                    let mut path = vec![first.unraw().to_string()];
                    let span = first.span();
                    while let [
                        proc_macro2::TokenTree::Punct(a),
                        proc_macro2::TokenTree::Punct(b),
                        proc_macro2::TokenTree::Ident(next),
                        ..,
                    ] = &tokens[index + 1..]
                    {
                        if a.as_char() != ':' || b.as_char() != ':' {
                            break;
                        }
                        path.push(next.unraw().to_string());
                        index += 3;
                    }
                    self.inspect(&path, span);
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn report(&mut self, span: Span, dependency: &str, parse_error: bool) {
        let location = span.start();
        let message = if parse_error {
            "Cannot verify the widget dependency boundary: Rust parsing failed".to_owned()
        } else {
            format!("Shared infrastructure depends on widget implementation `{dependency}`")
        };
        let mut violation = finding(
            SMELL,
            Severity::Error,
            self.path,
            location.line as u32,
            location.column as u32,
            &message,
            "Keep derived state with its infrastructure owner and pass generic inputs; review exact integration symbols separately",
        );
        violation.category = SmellCategory::Couplers;
        self.findings.push(violation);
    }
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        self.enter(file.items.iter(), true);
        syn::visit::visit_file(self, file);
        self.scopes.pop();
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if !test_only(item_attributes(item)) {
            syn::visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if let Some((_, items)) = &module.content {
            self.module.push(module.ident.unraw().to_string());
            self.enter(items.iter(), true);
            for item in items {
                self.visit_item(item);
            }
            self.scopes.pop();
            self.module.pop();
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.enter(
            block.stmts.iter().filter_map(|statement| {
                if let syn::Stmt::Item(item) = statement {
                    Some(item)
                } else {
                    None
                }
            }),
            false,
        );
        syn::visit::visit_block(self, block);
        self.scopes.pop();
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        imports(&item.tree, &[], &mut leaves);
        for leaf in leaves {
            self.inspect(&leaf.path, leaf.span);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.inspect(
            &path
                .segments
                .iter()
                .map(|segment| segment.ident.unraw().to_string())
                .collect::<Vec<_>>(),
            path.span(),
        );
        syn::visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.visit_path(&mac.path);
        self.inspect_tokens(mac.tokens.clone());
    }

    fn visit_meta_list(&mut self, meta: &'ast syn::MetaList) {
        self.visit_path(&meta.path);
        self.inspect_tokens(meta.tokens.clone());
    }
}

#[cfg(test)]
mod tests;
