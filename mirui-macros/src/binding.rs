//! Shared binding-role IR for authoring attribute macros.
//!
//! Role macros are syntax consumed by an enclosing attribute macro. They are
//! never emitted as standalone macro invocations.

#![allow(dead_code)]

use proc_macro2::Span;
use quote::ToTokens;
use std::collections::HashSet;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{FnArg, Ident, Macro, Member, Pat, Type};

mod keyword {
    syn::custom_keyword!(optional);
    syn::custom_keyword!(with);
    syn::custom_keyword!(without);
}

const MAX_QUERY_ARITY: usize = 8;
const MAX_SYSTEM_FETCH_ARITY: usize = 8;
const MAX_ANIMATE_COMPONENTS: usize = MAX_QUERY_ARITY - 2;

/// One authored parameter after its role syntax has been normalized.
#[derive(Clone)]
pub(crate) struct BindingSpec {
    pub(crate) name: Ident,
    /// The complete authored role syntax and its diagnostic span.
    pub(crate) syntax: BindingSyntax,
    pub(crate) logical_type: LogicalType,
    pub(crate) source: BindingSource,
    pub(crate) access: BindingAccess,
    pub(crate) cardinality: BindingCardinality,
    /// Named Model getters observed by a View binding.
    pub(crate) watch_selectors: Vec<Ident>,
    /// Dirty policy applied when an observed value changes.
    pub(crate) invalidation: Option<BindingInvalidation>,
    pub(crate) ownership: BindingOwnership,
}

/// Original syntax retained separately from normalized binding semantics.
#[derive(Clone)]
pub(crate) enum BindingSyntax {
    TypeRole(syn::TypeMacro),
    ExprRole(syn::ExprMacro),
    Generated(Span),
}

impl BindingSyntax {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::TypeRole(role) => role.span(),
            Self::ExprRole(role) => role.span(),
            Self::Generated(span) => *span,
        }
    }
}

/// The value type exposed to authored code.
///
/// Queries and intrinsic capabilities describe their value shape in
/// `BindingSource`, so they use `SourceDefined` instead of inventing a scalar
/// type here.
#[derive(Clone)]
pub(crate) enum LogicalType {
    Explicit(Type),
    InferredFrom { binding: Ident, field: Member },
    Intrinsic(BindingCapability),
    QueryRow(QueryRowShape),
}

/// Runtime domain from which a binding obtains its value.
#[derive(Clone)]
pub(crate) enum BindingSource {
    Model(ModelSource),
    Resource(ResourceSource),
    Component(ComponentSource),
    Query(QuerySpec),
    Event,
    Capability(BindingCapability),
}

#[derive(Clone)]
pub(crate) enum ModelSource {
    Registered { ty: Type },
    Projection { binding: Ident, field: Member },
}

#[derive(Clone)]
pub(crate) struct ResourceSource {
    pub(crate) ty: Type,
}

#[derive(Clone)]
pub(crate) struct ComponentSource {
    pub(crate) ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingCapability {
    Commands,
    Entity,
    Paint,
    Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoleKind {
    Model,
    Res,
    Query,
    Commands,
    Component,
    Entity,
    Paint,
    Motion,
}

impl RoleKind {
    fn from_macro(role: &Macro) -> Option<Self> {
        let name = role.path.get_ident()?;
        Some(match name.to_string().as_str() {
            "model" => Self::Model,
            "res" => Self::Res,
            "query" => Self::Query,
            "commands" => Self::Commands,
            "component" => Self::Component,
            "entity" => Self::Entity,
            "paint" => Self::Paint,
            "motion" => Self::Motion,
            _ => return None,
        })
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Res => "res",
            Self::Query => "query",
            Self::Commands => "commands",
            Self::Component => "component",
            Self::Entity => "entity",
            Self::Paint => "paint",
            Self::Motion => "motion",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RoleSet {
    roles: &'static [RoleKind],
}

pub(crate) const COMPONENT_FIELD_ROLES: RoleSet = RoleSet::new(&[RoleKind::Model]);
pub(crate) const COMPOSE_ROLES: RoleSet = RoleSet::new(&[RoleKind::Model]);
pub(crate) const VIEW_ROLES: RoleSet = RoleSet::new(&[
    RoleKind::Component,
    RoleKind::Model,
    RoleKind::Res,
    RoleKind::Entity,
    RoleKind::Paint,
]);
pub(crate) const SYSTEM_ROLES: RoleSet = RoleSet::new(&[
    RoleKind::Model,
    RoleKind::Res,
    RoleKind::Query,
    RoleKind::Commands,
]);
pub(crate) const TIMER_ROLES: RoleSet =
    RoleSet::new(&[RoleKind::Model, RoleKind::Entity, RoleKind::Commands]);
pub(crate) const ANIMATE_ROLES: RoleSet = RoleSet::new(&[
    RoleKind::Model,
    RoleKind::Entity,
    RoleKind::Motion,
    RoleKind::Component,
    RoleKind::Commands,
]);

impl RoleSet {
    const fn new(roles: &'static [RoleKind]) -> Self {
        Self { roles }
    }

    pub(crate) fn classify(
        self,
        ty: &Type,
        context: &str,
    ) -> syn::Result<Option<(RoleKind, syn::TypeMacro)>> {
        let Type::Macro(type_role) = ty else {
            return Ok(None);
        };
        let Some(kind) = RoleKind::from_macro(&type_role.mac) else {
            return Ok(None);
        };
        self.require(kind, &type_role.mac, context)?;
        require_parentheses(&type_role.mac)?;
        Ok(Some((kind, type_role.clone())))
    }

    pub(crate) fn parse_required(
        self,
        ty: &Type,
        context: &str,
        subject: &str,
    ) -> syn::Result<(RoleKind, syn::TypeMacro)> {
        let Type::Macro(type_role) = ty else {
            return Err(syn::Error::new_spanned(
                ty,
                format!("{subject} must use {}", self.expected()),
            ));
        };
        let Some(kind) = RoleKind::from_macro(&type_role.mac) else {
            let path = type_role.mac.path.to_token_stream().to_string();
            return Err(syn::Error::new_spanned(
                &type_role.mac.path,
                format!(
                    "unknown binding role `{path}!` in `{context}`; expected {}",
                    self.expected()
                ),
            ));
        };
        self.require(kind, &type_role.mac, context)?;
        require_parentheses(&type_role.mac)?;
        Ok((kind, type_role.clone()))
    }

    fn require(self, kind: RoleKind, role: &Macro, context: &str) -> syn::Result<()> {
        if self.roles.contains(&kind) {
            return Ok(());
        }
        Err(syn::Error::new_spanned(
            &role.path,
            format!(
                "binding role `{}!` is not available in `{context}`; expected {}",
                kind.name(),
                self.expected()
            ),
        ))
    }

    fn expected(self) -> String {
        self.roles
            .iter()
            .map(|kind| format!("`{}!`", kind.name()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingAccess {
    Read,
    Write,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingCardinality {
    Required,
    Optional,
    Many,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingInvalidation {
    Visual,
    Layout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingOwnership {
    BorrowedForCall,
    CapturedShared,
    Deferred,
}

/// Hierarchical query shape retained for typed-query lowering.
#[derive(Clone)]
pub(crate) struct QuerySpec {
    pub(crate) items: Vec<QueryItemSpec>,
    pub(crate) filters: Vec<QueryFilterSpec>,
    /// The first required component item, otherwise the first `with` filter.
    pub(crate) driver: QueryDriver,
    pub(crate) row_shape: QueryRowShape,
}

impl QuerySpec {
    pub(crate) fn arity(&self) -> usize {
        self.items.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryRowShape {
    Scalar,
    Tuple(usize),
}

#[derive(Clone)]
pub(crate) enum QueryItemSpec {
    Entity(Ident),
    Component {
        ty: Box<Type>,
        access: BindingAccess,
        cardinality: BindingCardinality,
    },
}

#[derive(Clone)]
pub(crate) struct QueryFilterSpec {
    pub(crate) kind: QueryFilterKind,
    pub(crate) ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryFilterKind {
    With,
    Without,
}

/// Index into the matching query item or filter vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryDriver {
    Item(usize),
    WithFilter(usize),
}

impl BindingSpec {
    /// Parse a direct `model!(...)` type role for authoring frontends that
    /// also accept ordinary Rust types.
    ///
    /// `Ok(None)` means the type is not a Model role. Context-specific
    /// frontends still decide whether registered and projected Model sources
    /// are legal in that position.
    pub(crate) fn parse_model_type_role(name: Ident, ty: &Type) -> syn::Result<Option<Self>> {
        let Type::Macro(type_role) = ty else {
            return Ok(None);
        };
        if RoleKind::from_macro(&type_role.mac) != Some(RoleKind::Model) {
            return Ok(None);
        }
        require_parentheses(&type_role.mac)?;
        parse_model_binding(name, type_role.clone()).map(Some)
    }

    /// Parse and validate the complete parameter list for one `#[animate]`.
    pub(crate) fn parse_animate_parameters(
        inputs: &Punctuated<FnArg, syn::Token![,]>,
    ) -> syn::Result<Vec<Self>> {
        let mut bindings = Vec::with_capacity(inputs.len());
        let mut names = HashSet::with_capacity(inputs.len());
        let mut component_types = HashSet::new();
        let mut component_count = 0usize;
        let mut entity_seen = false;
        let mut motion_seen = false;
        let mut commands_seen = false;

        for input in inputs {
            let binding = Self::parse_animate_parameter(input)?;
            if !names.insert(binding.name.to_string()) {
                return Err(syn::Error::new(
                    binding.name.span(),
                    format!(
                        "animate binding `{}` is declared more than once",
                        binding.name
                    ),
                ));
            }
            match &binding.source {
                BindingSource::Component(component) => {
                    if component_count == MAX_ANIMATE_COMPONENTS {
                        return Err(syn::Error::new(
                            binding.syntax.span(),
                            format!(
                                "`#[animate]` supports at most {MAX_ANIMATE_COMPONENTS} component parameters",
                            ),
                        ));
                    }
                    component_count += 1;
                    let key = component.ty.to_token_stream().to_string();
                    if !component_types.insert(key) {
                        return Err(syn::Error::new_spanned(
                            &component.ty,
                            "an animated entity component can only be bound once",
                        ));
                    }
                }
                BindingSource::Capability(capability) => {
                    let (seen, role) = match capability {
                        BindingCapability::Entity => (&mut entity_seen, "entity"),
                        BindingCapability::Motion => (&mut motion_seen, "motion"),
                        BindingCapability::Commands => (&mut commands_seen, "commands"),
                        _ => {
                            return Err(syn::Error::new(
                                binding.syntax.span(),
                                "binding capability is not available in `#[animate]`",
                            ));
                        }
                    };
                    if core::mem::replace(seen, true) {
                        return Err(syn::Error::new(
                            binding.syntax.span(),
                            format!("`#[animate]` accepts at most one `{role}!()` parameter"),
                        ));
                    }
                }
                _ => {}
            }
            bindings.push(binding);
        }
        Ok(bindings)
    }

    fn parse_animate_parameter(input: &FnArg) -> syn::Result<Self> {
        let FnArg::Typed(parameter) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "animations cannot have a receiver; use a named binding parameter",
            ));
        };
        if let Some(attribute) = parameter.attrs.first() {
            return Err(syn::Error::new_spanned(
                attribute,
                "animate binding parameters do not accept attributes",
            ));
        }
        let Pat::Ident(pattern) = &*parameter.pat else {
            return Err(syn::Error::new_spanned(
                &parameter.pat,
                "animate binding parameters must use a plain identifier",
            ));
        };
        if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
            return Err(syn::Error::new_spanned(
                &parameter.pat,
                "animate binding parameters must use a plain identifier",
            ));
        }
        let (role, type_role) =
            ANIMATE_ROLES.parse_required(&parameter.ty, "#[animate]", "animate parameters")?;
        let name = pattern.ident.clone();
        match role {
            RoleKind::Model => {
                let binding = parse_model_binding(name, type_role)?;
                if !matches!(
                    &binding.source,
                    BindingSource::Model(ModelSource::Registered { .. })
                ) || binding.cardinality != BindingCardinality::Required
                {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        "Animate Model bindings capture registered instances; use `model!(T)`",
                    ));
                }
                Ok(binding)
            }
            RoleKind::Component => parse_animate_component(name, type_role),
            RoleKind::Entity => {
                parse_animate_capability(name, type_role, BindingCapability::Entity)
            }
            RoleKind::Motion => {
                parse_animate_capability(name, type_role, BindingCapability::Motion)
            }
            RoleKind::Commands => {
                parse_animate_capability(name, type_role, BindingCapability::Commands)
            }
            _ => unreachable!("RoleSet admitted an unsupported animate role"),
        }
    }

    /// Parse and validate the complete parameter list for one `#[system]`.
    ///
    /// Syntax and context validation happen here. Rust type checking validates
    /// component/model traits, scheduler registration validates aliases after
    /// type normalization, and callback preparation handles missing required
    /// resources.
    pub(crate) fn parse_system_parameters(
        inputs: &Punctuated<FnArg, syn::Token![,]>,
    ) -> syn::Result<Vec<Self>> {
        let mut bindings = Vec::with_capacity(inputs.len());
        let mut names = HashSet::with_capacity(inputs.len());
        let mut fetch_count = 0usize;
        let mut commands_seen = false;

        for input in inputs {
            let binding = Self::parse_system_parameter(input)?;
            if !names.insert(binding.name.to_string()) {
                return Err(syn::Error::new(
                    binding.name.span(),
                    format!(
                        "system binding `{}` is declared more than once",
                        binding.name
                    ),
                ));
            }
            match &binding.source {
                BindingSource::Resource(_) | BindingSource::Query(_) => {
                    if fetch_count == MAX_SYSTEM_FETCH_ARITY {
                        return Err(syn::Error::new(
                            binding.syntax.span(),
                            format!(
                                "`#[system]` supports at most {MAX_SYSTEM_FETCH_ARITY} resource/query parameters",
                            ),
                        ));
                    }
                    fetch_count += 1;
                }
                BindingSource::Capability(BindingCapability::Commands) => {
                    if commands_seen {
                        return Err(syn::Error::new(
                            binding.syntax.span(),
                            "`#[system]` accepts at most one `commands!()` parameter",
                        ));
                    }
                    commands_seen = true;
                }
                _ => {}
            }
            bindings.push(binding);
        }
        Ok(bindings)
    }

    /// Parse one parameter from the frozen `#[system]` role grammar.
    pub(crate) fn parse_system_parameter(input: &FnArg) -> syn::Result<Self> {
        let FnArg::Typed(parameter) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "systems cannot have a receiver; use a named binding parameter",
            ));
        };
        if let Some(attribute) = parameter.attrs.first() {
            return Err(syn::Error::new_spanned(
                attribute,
                "system binding parameters do not accept attributes",
            ));
        }
        let Pat::Ident(pattern) = &*parameter.pat else {
            return Err(syn::Error::new_spanned(
                &parameter.pat,
                "system binding parameters must use a plain identifier",
            ));
        };
        if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
            return Err(syn::Error::new_spanned(
                &parameter.pat,
                "system binding parameters must use a plain identifier",
            ));
        }

        let (role, type_role) =
            SYSTEM_ROLES.parse_required(&parameter.ty, "#[system]", "system parameters")?;

        let name = pattern.ident.clone();
        match role {
            RoleKind::Model => parse_system_model(name, type_role),
            RoleKind::Res => parse_system_resource(name, type_role),
            RoleKind::Query => parse_system_query(name, type_role),
            RoleKind::Commands => parse_system_commands(name, type_role),
            _ => unreachable!("RoleSet admitted an unsupported system role"),
        }
    }
}

fn parse_system_model(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let binding = parse_model_binding(name, type_role)?;
    if !matches!(
        &binding.source,
        BindingSource::Model(ModelSource::Registered { .. })
    ) || binding.cardinality != BindingCardinality::Required
    {
        return Err(syn::Error::new(
            binding.syntax.span(),
            "system model bindings are required registered instances; use `model!(T)`",
        ));
    }
    Ok(binding)
}

fn parse_model_binding(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let parsed = type_role.mac.parse_body::<ModelRole>()?;
    let syntax = BindingSyntax::TypeRole(type_role);
    Ok(match parsed {
        ModelRole::Declared { ty, cardinality } => BindingSpec {
            name,
            syntax,
            logical_type: LogicalType::Explicit(ty.clone()),
            source: BindingSource::Model(ModelSource::Registered { ty }),
            access: BindingAccess::Read,
            cardinality,
            watch_selectors: Vec::new(),
            invalidation: None,
            ownership: BindingOwnership::CapturedShared,
        },
        ModelRole::Projection {
            binding,
            field,
            cardinality,
            watch_selectors,
        } => BindingSpec {
            name,
            syntax,
            logical_type: LogicalType::InferredFrom {
                binding: binding.clone(),
                field: field.clone(),
            },
            source: BindingSource::Model(ModelSource::Projection { binding, field }),
            access: BindingAccess::Read,
            cardinality,
            watch_selectors,
            invalidation: None,
            ownership: BindingOwnership::BorrowedForCall,
        },
    })
}

struct AnimateComponentRole {
    ty: Type,
    access: BindingAccess,
}

impl Parse for AnimateComponentRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error("`component!` expects a component type"));
        }
        if input.peek(keyword::optional) {
            let optional = input.parse::<keyword::optional>()?;
            return Err(syn::Error::new(
                optional.span(),
                "`#[animate]` component bindings are required",
            ));
        }
        let access = if input.peek(syn::Token![mut]) {
            input.parse::<syn::Token![mut]>()?;
            BindingAccess::Write
        } else {
            BindingAccess::Read
        };
        let ty = input.parse::<Type>()?;
        reject_primitive_component_type(&ty)?;
        if !input.is_empty() {
            return Err(input.error("`component!` expects exactly one component type"));
        }
        Ok(Self { ty, access })
    }
}

fn reject_primitive_component_type(ty: &Type) -> syn::Result<()> {
    let Type::Path(path) = ty else {
        return Ok(());
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return Ok(());
    }
    let segment = &path.path.segments[0];
    let primitive = matches!(
        segment.ident.to_string().as_str(),
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    );
    if primitive && segment.arguments.is_empty() {
        return Err(syn::Error::new_spanned(
            ty,
            format!(
                "`{}` is a primitive type and cannot be used as an animated component",
                segment.ident
            ),
        ));
    }
    Ok(())
}

fn parse_animate_component(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let parsed = type_role.mac.parse_body::<AnimateComponentRole>()?;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::Explicit(parsed.ty.clone()),
        source: BindingSource::Component(ComponentSource { ty: parsed.ty }),
        access: parsed.access,
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership: BindingOwnership::BorrowedForCall,
    })
}

fn parse_animate_capability(
    name: Ident,
    type_role: syn::TypeMacro,
    capability: BindingCapability,
) -> syn::Result<BindingSpec> {
    if !type_role.mac.tokens.is_empty() {
        return Err(syn::Error::new_spanned(
            &type_role.mac.tokens,
            format!(
                "`{}!()` does not accept arguments",
                capability_name(capability)
            ),
        ));
    }
    let access = if capability == BindingCapability::Commands {
        BindingAccess::Command
    } else {
        BindingAccess::Read
    };
    let ownership = if capability == BindingCapability::Commands {
        BindingOwnership::Deferred
    } else {
        BindingOwnership::BorrowedForCall
    };
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::Intrinsic(capability),
        source: BindingSource::Capability(capability),
        access,
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership,
    })
}

fn capability_name(capability: BindingCapability) -> &'static str {
    match capability {
        BindingCapability::Commands => "commands",
        BindingCapability::Entity => "entity",
        BindingCapability::Paint => "paint",
        BindingCapability::Motion => "motion",
    }
}

fn parse_system_resource(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let parsed = type_role.mac.parse_body::<ResourceRole>()?;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::Explicit(parsed.ty.clone()),
        source: BindingSource::Resource(ResourceSource { ty: parsed.ty }),
        access: parsed.access,
        cardinality: parsed.cardinality,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership: BindingOwnership::BorrowedForCall,
    })
}

fn parse_system_query(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    let query = type_role.mac.parse_body::<QuerySpec>()?;
    let access = if query.items.iter().any(|item| {
        matches!(
            item,
            QueryItemSpec::Component {
                access: BindingAccess::Write,
                ..
            }
        )
    }) {
        BindingAccess::Write
    } else {
        BindingAccess::Read
    };
    let row_shape = query.row_shape;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::QueryRow(row_shape),
        source: BindingSource::Query(query),
        access,
        cardinality: BindingCardinality::Many,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership: BindingOwnership::BorrowedForCall,
    })
}

fn parse_system_commands(name: Ident, type_role: syn::TypeMacro) -> syn::Result<BindingSpec> {
    type_role.mac.parse_body::<EmptyCommandsRole>()?;
    Ok(BindingSpec {
        name,
        syntax: BindingSyntax::TypeRole(type_role),
        logical_type: LogicalType::Intrinsic(BindingCapability::Commands),
        source: BindingSource::Capability(BindingCapability::Commands),
        access: BindingAccess::Command,
        cardinality: BindingCardinality::Required,
        watch_selectors: Vec::new(),
        invalidation: None,
        ownership: BindingOwnership::Deferred,
    })
}

fn require_parentheses(role: &Macro) -> syn::Result<()> {
    if matches!(role.delimiter, syn::MacroDelimiter::Paren(_)) {
        return Ok(());
    }
    Err(syn::Error::new(
        role.delimiter.span().open(),
        "binding roles use parentheses, for example `res!(Time)`",
    ))
}

enum ModelRole {
    Declared {
        ty: Type,
        cardinality: BindingCardinality,
    },
    Projection {
        binding: Ident,
        field: Member,
        cardinality: BindingCardinality,
        watch_selectors: Vec<Ident>,
    },
}

impl Parse for ModelRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error("`model!` expects a model type or component field projection"));
        }
        if input.peek(keyword::optional) {
            input.parse::<keyword::optional>()?;
            if input.peek(syn::Token![mut]) {
                let mutable = input.parse::<syn::Token![mut]>()?;
                return Err(syn::Error::new(
                    mutable.span,
                    "optional mutable model bindings are not supported",
                ));
            }
            if is_projection(input) {
                return parse_model_projection(input, BindingCardinality::Optional);
            }
            let ty = input.parse::<Type>()?;
            if !input.is_empty() {
                return Err(input.error("`model!` expects exactly one model type"));
            }
            return Ok(Self::Declared {
                ty,
                cardinality: BindingCardinality::Optional,
            });
        }

        if is_projection(input) {
            return parse_model_projection(input, BindingCardinality::Required);
        }

        let ty = input.parse::<Type>()?;
        if !input.is_empty() {
            return Err(input.error("`model!` expects exactly one model type"));
        }
        Ok(Self::Declared {
            ty,
            cardinality: BindingCardinality::Required,
        })
    }
}

fn is_projection(input: ParseStream<'_>) -> bool {
    let projection = input.fork();
    projection.parse::<Ident>().is_ok() && projection.peek(syn::Token![.])
}

fn parse_model_projection(
    input: ParseStream<'_>,
    cardinality: BindingCardinality,
) -> syn::Result<ModelRole> {
    let binding = input.parse::<Ident>()?;
    input.parse::<syn::Token![.]>()?;
    let field = Member::Named(input.parse::<Ident>()?);
    let mut watch_selectors = Vec::new();
    if input.peek(syn::Token![,]) {
        input.parse::<syn::Token![,]>()?;
        let watch = input.parse::<Ident>()?;
        if watch != "watch" {
            return Err(syn::Error::new(
                watch.span(),
                "model field projections only accept `watch(...)` after the field",
            ));
        }
        let content;
        syn::parenthesized!(content in input);
        let selectors = content.parse_terminated(Ident::parse, syn::Token![,])?;
        if selectors.is_empty() {
            return Err(content.error("`watch(...)` requires at least one getter"));
        }
        for selector in selectors {
            if watch_selectors.iter().any(|current| current == &selector) {
                return Err(syn::Error::new(selector.span(), "duplicate watch selector"));
            }
            watch_selectors.push(selector);
        }
    }
    if !input.is_empty() {
        return Err(input.error("unexpected tokens after model field projection"));
    }
    Ok(ModelRole::Projection {
        binding,
        field,
        cardinality,
        watch_selectors,
    })
}

struct EmptyCommandsRole;

impl Parse for EmptyCommandsRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            Ok(Self)
        } else {
            Err(input.error("`commands!` does not accept arguments; use `commands!()`"))
        }
    }
}

struct ResourceRole {
    ty: Type,
    access: BindingAccess,
    cardinality: BindingCardinality,
}

impl Parse for ResourceRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error("`res!` expects a resource type"));
        }
        let (access, cardinality) = if input.peek(keyword::optional) {
            input.parse::<keyword::optional>()?;
            if input.peek(syn::Token![mut]) {
                let mutable = input.parse::<syn::Token![mut]>()?;
                return Err(syn::Error::new(
                    mutable.span,
                    "optional mutable resources are not supported; use `res!(optional T)` or `res!(mut T)`",
                ));
            }
            (BindingAccess::Read, BindingCardinality::Optional)
        } else if input.peek(syn::Token![mut]) {
            let mutable = input.parse::<syn::Token![mut]>()?;
            if input.peek(keyword::optional) {
                return Err(syn::Error::new(
                    mutable.span,
                    "optional mutable resources are not supported; use `res!(optional T)` or `res!(mut T)`",
                ));
            }
            (BindingAccess::Write, BindingCardinality::Required)
        } else {
            (BindingAccess::Read, BindingCardinality::Required)
        };
        let ty = input.parse::<Type>()?;
        if !input.is_empty() {
            return Err(input.error("`res!` expects exactly one resource type"));
        }
        Ok(Self {
            ty,
            access,
            cardinality,
        })
    }
}

impl Parse for QuerySpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut items = Vec::new();
        let mut entity_seen = false;
        while !input.is_empty() && !input.peek(syn::Token![;]) {
            let item = parse_query_item(input)?;
            if items.len() == MAX_QUERY_ARITY {
                return Err(syn::Error::new(
                    query_item_span(&item),
                    format!("`query!` supports at most {MAX_QUERY_ARITY} returned items"),
                ));
            }
            if let QueryItemSpec::Entity(entity) = &item {
                if entity_seen {
                    return Err(syn::Error::new(
                        entity.span(),
                        "`Entity` may appear at most once in a query",
                    ));
                }
                entity_seen = true;
            }
            items.push(item);

            if input.is_empty() || input.peek(syn::Token![;]) {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }

        if items.is_empty() {
            return Err(input.error("`query!` requires at least one returned item"));
        }

        let mut filters = Vec::new();
        if input.peek(syn::Token![;]) {
            input.parse::<syn::Token![;]>()?;
        }
        while !input.is_empty() {
            let (kind, keyword_span) = if input.peek(keyword::with) {
                let keyword = input.parse::<keyword::with>()?;
                (QueryFilterKind::With, keyword.span)
            } else if input.peek(keyword::without) {
                let keyword = input.parse::<keyword::without>()?;
                (QueryFilterKind::Without, keyword.span)
            } else {
                return Err(syn::Error::new(
                    input.span(),
                    "unknown query filter; expected `with T;` or `without T;`",
                ));
            };
            let ty = input.parse::<Type>().map_err(|error| {
                syn::Error::new(
                    error.span(),
                    "query filters require a component type after `with` or `without`",
                )
            })?;
            if filters.len() == MAX_QUERY_ARITY {
                return Err(syn::Error::new(
                    keyword_span,
                    format!("`query!` supports at most {MAX_QUERY_ARITY} filters"),
                ));
            }
            reject_duplicate_filter(&filters, kind, &ty)?;
            filters.push(QueryFilterSpec { kind, ty });
            if !input.peek(syn::Token![;]) {
                return Err(input.error("query filters must end with `;`"));
            }
            input.parse::<syn::Token![;]>()?;
        }

        let driver = items
            .iter()
            .enumerate()
            .find_map(|(index, item)| match item {
                QueryItemSpec::Component {
                    cardinality: BindingCardinality::Required,
                    ..
                } => Some(QueryDriver::Item(index)),
                _ => None,
            })
            .or_else(|| {
                filters
                    .iter()
                    .enumerate()
                    .find_map(|(index, filter)| {
                        (filter.kind == QueryFilterKind::With)
                            .then_some(QueryDriver::WithFilter(index))
                    })
            })
            .ok_or_else(|| {
                syn::Error::new(
                    query_items_span(&items),
                    "`query!` needs a required component item or `with` filter to drive iteration; `Entity`, optional items, and `without` filters cannot drive a query",
                )
            })?;
        let row_shape = if items.len() == 1 {
            QueryRowShape::Scalar
        } else {
            QueryRowShape::Tuple(items.len())
        };

        Ok(Self {
            items,
            filters,
            driver,
            row_shape,
        })
    }
}

fn parse_query_item(input: ParseStream<'_>) -> syn::Result<QueryItemSpec> {
    let (access, cardinality) = if input.peek(keyword::optional) {
        input.parse::<keyword::optional>()?;
        if input.peek(syn::Token![mut]) {
            let mutable = input.parse::<syn::Token![mut]>()?;
            return Err(syn::Error::new(
                mutable.span,
                "optional mutable query items are not supported; use `optional T` or `mut T`",
            ));
        }
        (BindingAccess::Read, BindingCardinality::Optional)
    } else if input.peek(syn::Token![mut]) {
        let mutable = input.parse::<syn::Token![mut]>()?;
        if input.peek(keyword::optional) {
            return Err(syn::Error::new(
                mutable.span,
                "optional mutable query items are not supported; use `optional T` or `mut T`",
            ));
        }
        (BindingAccess::Write, BindingCardinality::Required)
    } else {
        (BindingAccess::Read, BindingCardinality::Required)
    };
    let ty = input.parse::<Type>()?;
    if let Some(entity) = entity_ident(&ty) {
        if access != BindingAccess::Read || cardinality != BindingCardinality::Required {
            return Err(syn::Error::new_spanned(
                ty,
                "`Entity` cannot be `mut` or `optional` in a query",
            ));
        }
        Ok(QueryItemSpec::Entity(entity.clone()))
    } else {
        Ok(QueryItemSpec::Component {
            ty: Box::new(ty),
            access,
            cardinality,
        })
    }
}

fn entity_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() || path.path.leading_colon.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    let segment = path.path.segments.first()?;
    matches!(segment.arguments, syn::PathArguments::None)
        .then_some(&segment.ident)
        .filter(|ident| *ident == "Entity")
}

fn reject_duplicate_filter(
    filters: &[QueryFilterSpec],
    kind: QueryFilterKind,
    ty: &Type,
) -> syn::Result<()> {
    let key = ty.to_token_stream().to_string();
    let Some(previous) = filters
        .iter()
        .find(|filter| filter.ty.to_token_stream().to_string() == key)
    else {
        return Ok(());
    };

    let message = if previous.kind == kind {
        let keyword = match kind {
            QueryFilterKind::With => "with",
            QueryFilterKind::Without => "without",
        };
        format!("duplicate `{keyword}` filter for `{key}`")
    } else {
        format!("query component `{key}` cannot appear in both `with` and `without` filters")
    };
    Err(syn::Error::new_spanned(ty, message))
}

fn query_item_span(item: &QueryItemSpec) -> Span {
    match item {
        QueryItemSpec::Entity(entity) => entity.span(),
        QueryItemSpec::Component { ty, .. } => ty.span(),
    }
}

fn query_items_span(items: &[QueryItemSpec]) -> Span {
    items
        .first()
        .map(query_item_span)
        .unwrap_or_else(Span::call_site)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters(source: &str) -> Punctuated<FnArg, syn::Token![,]> {
        syn::parse_str::<syn::ItemFn>(&format!("fn system({source}) {{}}"))
            .unwrap_or_else(|error| panic!("invalid test parameters `{source}`: {error}"))
            .sig
            .inputs
    }

    fn parameter(source: &str) -> FnArg {
        parameters(source).into_iter().next().unwrap()
    }

    fn parse(source: &str) -> BindingSpec {
        BindingSpec::parse_system_parameter(&parameter(source))
            .unwrap_or_else(|error| panic!("failed to parse `{source}`: {error}"))
    }

    fn animate(source: &str) -> Vec<BindingSpec> {
        BindingSpec::parse_animate_parameters(&parameters(source))
            .unwrap_or_else(|error| panic!("failed to parse animate bindings `{source}`: {error}"))
    }

    fn animate_error(source: &str) -> String {
        match BindingSpec::parse_animate_parameters(&parameters(source)) {
            Ok(_) => panic!("expected animate bindings `{source}` to fail"),
            Err(error) => error.to_string(),
        }
    }

    fn diagnostic(source: &str) -> syn::Error {
        match BindingSpec::parse_system_parameter(&parameter(source)) {
            Ok(_) => panic!("expected `{source}` to fail"),
            Err(error) => error,
        }
    }

    fn error(source: &str) -> String {
        diagnostic(source).to_string()
    }

    fn tokens<T: ToTokens>(value: &T) -> String {
        value.to_token_stream().to_string()
    }

    fn syntax_tokens(syntax: &BindingSyntax) -> String {
        match syntax {
            BindingSyntax::TypeRole(role) => tokens(role),
            BindingSyntax::ExprRole(role) => tokens(role),
            BindingSyntax::Generated(_) => "<generated>".into(),
        }
    }

    #[test]
    fn parses_all_system_roles_into_shared_ir() {
        let model = parse("game: model!(GameModel)");
        assert_eq!(model.name, "game");
        assert!(matches!(
            &model.source,
            BindingSource::Model(ModelSource::Registered { ty }) if tokens(ty) == "GameModel"
        ));
        assert!(matches!(
            model.logical_type,
            LogicalType::Explicit(ref ty) if tokens(ty) == "GameModel"
        ));
        assert_eq!(model.access, BindingAccess::Read);
        assert_eq!(model.cardinality, BindingCardinality::Required);
        assert_eq!(model.ownership, BindingOwnership::CapturedShared);
        assert_eq!(syntax_tokens(&model.syntax), "model ! (GameModel)");
        assert!(model.watch_selectors.is_empty());
        assert_eq!(model.invalidation, None);

        let read = parse("time: res!(Time)");
        assert!(matches!(
            &read.source,
            BindingSource::Resource(ResourceSource { ty }) if tokens(ty) == "Time"
        ));
        assert_eq!(read.access, BindingAccess::Read);
        assert_eq!(read.cardinality, BindingCardinality::Required);

        let write = parse("settings: res!(mut Settings)");
        assert_eq!(write.access, BindingAccess::Write);
        assert_eq!(write.cardinality, BindingCardinality::Required);

        let optional = parse("metrics: res!(optional Metrics)");
        assert_eq!(optional.access, BindingAccess::Read);
        assert_eq!(optional.cardinality, BindingCardinality::Optional);

        let commands = parse("commands: commands!()");
        assert!(matches!(
            commands.source,
            BindingSource::Capability(BindingCapability::Commands)
        ));
        assert!(matches!(
            commands.logical_type,
            LogicalType::Intrinsic(BindingCapability::Commands)
        ));
        assert_eq!(commands.access, BindingAccess::Command);
        assert_eq!(commands.ownership, BindingOwnership::Deferred);
    }

    #[test]
    fn parses_all_animate_roles_into_shared_ir() {
        let bindings = animate(
            "game: model!(Game), entity: entity!(), value: motion!(), style: component!(mut Style), marker: component!(Marker), commands: commands!()",
        );
        assert!(matches!(
            bindings[0].source,
            BindingSource::Model(ModelSource::Registered { .. })
        ));
        assert!(matches!(
            bindings[1].source,
            BindingSource::Capability(BindingCapability::Entity)
        ));
        assert!(matches!(
            bindings[2].source,
            BindingSource::Capability(BindingCapability::Motion)
        ));
        assert!(matches!(bindings[3].source, BindingSource::Component(_)));
        assert_eq!(bindings[3].access, BindingAccess::Write);
        assert!(matches!(bindings[4].source, BindingSource::Component(_)));
        assert_eq!(bindings[4].access, BindingAccess::Read);
        assert!(matches!(
            bindings[5].source,
            BindingSource::Capability(BindingCapability::Commands)
        ));
    }

    #[test]
    fn animate_roles_reject_ambiguous_or_unsupported_access() {
        assert!(
            animate_error("first: motion!(), second: motion!()")
                .contains("at most one `motion!()`")
        );
        assert!(
            animate_error("first: component!(Style), second: component!(Style)")
                .contains("can only be bound once")
        );
        assert!(
            animate_error("style: component!(optional Style)")
                .contains("component bindings are required")
        );
        assert!(animate_error("game: model!(optional Game)").contains("use `model!(T)`"));
        assert!(animate_error("time: res!(Time)").contains("not available in `#[animate]`"));
    }

    #[test]
    fn model_ir_preserves_projection_watches_and_invalidation_separately() {
        let ty = syn::parse_str::<Type>("model!(board.game, watch(positions, selection))").unwrap();
        let Type::Macro(type_role) = ty else {
            panic!("expected role macro");
        };
        let mut binding =
            parse_model_binding(Ident::new("game", Span::call_site()), type_role).unwrap();
        binding.invalidation = Some(BindingInvalidation::Layout);

        assert!(matches!(
            &binding.logical_type,
            LogicalType::InferredFrom {
                binding,
                field: Member::Named(field),
            } if binding == "board" && field == "game"
        ));
        assert!(matches!(
            &binding.source,
            BindingSource::Model(ModelSource::Projection {
                binding,
                field: Member::Named(field),
            }) if binding == "board" && field == "game"
        ));
        assert_eq!(
            binding
                .watch_selectors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["positions", "selection"]
        );
        assert_eq!(binding.invalidation, Some(BindingInvalidation::Layout));
    }

    #[test]
    fn parses_query_items_filters_and_driver() {
        let binding = parse(
            "bars: query!(Entity, Transform, mut BouncingBar, optional Velocity; with Active; without Hidden;)",
        );
        assert_eq!(binding.access, BindingAccess::Write);
        assert_eq!(binding.cardinality, BindingCardinality::Many);
        let BindingSource::Query(query) = binding.source else {
            panic!("expected query source");
        };
        assert_eq!(query.arity(), 4);
        assert_eq!(query.filters.len(), 2);
        assert_eq!(query.driver, QueryDriver::Item(1));
        assert_eq!(query.row_shape, QueryRowShape::Tuple(4));
        assert!(matches!(query.items[0], QueryItemSpec::Entity(_)));
        assert!(matches!(
            &query.items[1],
            QueryItemSpec::Component {
                ty,
                access: BindingAccess::Read,
                cardinality: BindingCardinality::Required,
            } if tokens(ty) == "Transform"
        ));
        assert!(matches!(
            &query.items[2],
            QueryItemSpec::Component {
                ty,
                access: BindingAccess::Write,
                cardinality: BindingCardinality::Required,
            } if tokens(ty) == "BouncingBar"
        ));
        assert!(matches!(
            &query.items[3],
            QueryItemSpec::Component {
                ty,
                access: BindingAccess::Read,
                cardinality: BindingCardinality::Optional,
            } if tokens(ty) == "Velocity"
        ));
        assert_eq!(query.filters[0].kind, QueryFilterKind::With);
        assert_eq!(tokens(&query.filters[0].ty), "Active");
        assert_eq!(query.filters[1].kind, QueryFilterKind::Without);
        assert_eq!(tokens(&query.filters[1].ty), "Hidden");
    }

    #[test]
    fn with_filter_can_drive_entity_and_optional_items() {
        let binding = parse("rows: query!(Entity, optional Label; with Active;)");
        let BindingSource::Query(query) = binding.source else {
            panic!("expected query source");
        };
        assert_eq!(query.driver, QueryDriver::WithFilter(0));
        assert_eq!(query.row_shape, QueryRowShape::Tuple(2));
    }

    #[test]
    fn one_query_item_has_scalar_row_shape() {
        let binding = parse("positions: query!(Position)");
        let BindingSource::Query(query) = binding.source else {
            panic!("expected query source");
        };
        assert_eq!(query.row_shape, QueryRowShape::Scalar);
    }

    #[test]
    fn rejects_query_without_positive_driver() {
        let message = error("rows: query!(optional Label)");
        assert!(message.contains("needs a required component item or `with` filter"));
    }

    #[test]
    fn rejects_duplicate_and_conflicting_filters() {
        let duplicate = error("rows: query!(Entity; with Active; with Active;)");
        assert!(duplicate.contains("duplicate `with` filter for `Active`"));

        let conflict = error("rows: query!(Entity; with Active; without Active;)");
        assert!(conflict.contains("cannot appear in both `with` and `without`"));
    }

    #[test]
    fn rejects_optional_mut_access() {
        let resource = error("value: res!(optional mut Value)");
        assert!(resource.contains("optional mutable resources are not supported"));
        let reverse_resource = error("value: res!(mut optional Value)");
        assert!(reverse_resource.contains("optional mutable resources are not supported"));

        let query = error("rows: query!(optional mut Value; with Active;)");
        assert!(query.contains("optional mutable query items are not supported"));
        let reverse_query = error("rows: query!(mut optional Value; with Active;)");
        assert!(reverse_query.contains("optional mutable query items are not supported"));

        let pattern = error("mut value: res!(mut Value)");
        assert!(pattern.contains("binding parameters must use a plain identifier"));
    }

    #[test]
    fn rejects_query_item_and_filter_arity_above_eight() {
        let items = error("rows: query!(A, B, C, D, E, F, G, H, I)");
        assert!(items.contains("supports at most 8 returned items"));

        let filters = error(
            "rows: query!(Entity; with A; with B; with C; with D; with E; with F; with G; with H; with I;)",
        );
        assert!(filters.contains("supports at most 8 filters"));
    }

    #[test]
    fn rejects_empty_or_duplicate_entity_query_items() {
        assert!(error("rows: query!()").contains("requires at least one returned item"));
        assert!(
            error("rows: query!(Entity, Entity; with Active;)").contains("may appear at most once")
        );
    }

    #[test]
    fn rejects_unknown_or_context_invalid_roles() {
        let unknown = error("value: mystery!(Value)");
        assert!(unknown.contains("unknown binding role `mystery!` in `#[system]`"));

        let context = error("value: component!(Value)");
        assert!(context.contains("binding role `component!` is not available in `#[system]`"));

        let qualified = error("value: crate::res!(Value)");
        assert!(qualified.contains("unknown binding role `crate :: res!` in `#[system]`"));

        let wrapped = error("value: Option<model!(Value)>");
        assert!(wrapped.contains("system parameters must use"));
    }

    #[test]
    fn preserves_nested_authored_types_without_normalizing_option() {
        let binding = parse("game: model!(Option<Option<GameModel>>)");
        assert_eq!(
            syntax_tokens(&binding.syntax),
            "model ! (Option < Option < GameModel >>)"
        );
        assert!(matches!(
            binding.logical_type,
            LogicalType::Explicit(ref ty) if tokens(ty) == "Option < Option < GameModel > >"
        ));
    }

    #[test]
    fn commands_rejects_arguments_and_roles_require_parentheses() {
        let arguments = error("commands: commands!(World)");
        assert!(arguments.contains("does not accept arguments"));

        let delimiters = error("time: res![Time]");
        assert!(delimiters.contains("binding roles use parentheses"));
    }

    #[test]
    fn system_parameter_set_enforces_runtime_tuple_boundaries() {
        let too_many = parameters(
            "a: res!(A), b: res!(B), c: res!(C), d: res!(D), e: res!(E), f: res!(F), g: res!(G), h: res!(H), i: res!(I)",
        );
        let error = match BindingSpec::parse_system_parameters(&too_many) {
            Ok(_) => panic!("expected system fetch arity to fail"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("at most 8 resource/query parameters")
        );

        let duplicate_commands = parameters("a: commands!(), b: commands!()");
        let error = match BindingSpec::parse_system_parameters(&duplicate_commands) {
            Ok(_) => panic!("expected duplicate commands binding to fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("at most one `commands!()`"));

        let valid = parameters(
            "game: model!(Game), a: res!(A), b: res!(B), c: res!(C), d: res!(D), e: res!(E), f: res!(F), g: res!(G), h: query!(H), commands: commands!()",
        );
        assert_eq!(
            BindingSpec::parse_system_parameters(&valid).unwrap().len(),
            10
        );
    }

    #[test]
    fn diagnostics_point_at_the_offending_parser_token() {
        let cases = [
            ("value: mystery!(Value)", "mystery"),
            ("value: res!(optional mut Value)", "mut"),
            ("rows: query!(Entity; with Active; with Active;)", "Active"),
            ("rows: query!(A, B, C, D, E, F, G, H, I)", "I"),
        ];
        for (source, expected) in cases {
            let error = diagnostic(source);
            if let Some(actual) = error.span().source_text() {
                assert_eq!(actual, expected, "wrong diagnostic span for `{source}`");
            } else {
                // proc-macro2 omits source locations unless its optional
                // span-locations feature is enabled. The emitted diagnostic
                // must still retain the parser-selected span.
                assert!(
                    error
                        .to_compile_error()
                        .into_iter()
                        .all(|token| token.span().source_text().is_none())
                );
            }
        }
    }
}
