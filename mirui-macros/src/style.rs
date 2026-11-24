use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Expr, ExprLit, Lit};
use xrune::ds_node::ds_attr::DsAttrs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Common,
    Layout,
    Text,
    Button,
}

#[derive(Clone, Copy)]
enum Convert {
    Plain,
    Color,
    OptionalColor,
    FontStack,
    FontSize,
    Dimension,
    Fixed,
    FlexFactor,
}

#[derive(Clone, Copy)]
enum Binding {
    None,
    Reactive(&'static str),
    Responsive(&'static str),
    Both(&'static str),
}

#[derive(Clone, Copy)]
enum Change {
    Visual,
    Layout,
    FontStack,
    FontSize,
    TextColor,
}

pub(crate) struct Field {
    pub(crate) key: &'static str,
    alias: Option<&'static str>,
    pub(crate) target: Target,
    ty: &'static str,
    convert: Convert,
    binding: Binding,
    change: Change,
}

macro_rules! define_fields {
    ($( $target:ident $key:ident $(($alias:literal))?: $ty:ty => $convert:ident, $binding:expr $(, $change:ident)? );+ $(;)?) => {
        const FIELDS: &[Field] = &[
            $(Field {
                key: stringify!($key),
                alias: define_fields!(@alias $($alias)?),
                target: Target::$target,
                ty: stringify!($ty),
                convert: Convert::$convert,
                binding: $binding,
                change: define_fields!(@change $target $($change)?),
            },)+
        ];
    };
    (@change $target:ident $change:ident) => { Change::$change };
    (@change Layout) => { Change::Layout };
    (@change Text) => { Change::Layout };
    (@change $target:ident) => { Change::Visual };
    (@alias $alias:literal) => { Some($alias) };
    (@alias) => { None };
}

use Binding::{Both, None as Static, Reactive, Responsive};

define_fields! {
    Common bg_color: Option<ThemedColor> => OptionalColor, Both("BackgroundColor");
    Common border_color: Option<ThemedColor> => OptionalColor, Reactive("BorderColor");
    Common border_width: Fixed => Fixed, Reactive("BorderWidth");
    Common border_radius: Fixed => Fixed, Static, Layout;
    Common text_color: Option<ThemedColor> => OptionalColor, Both("TextColor"), TextColor;
    Common font_stack("font"): Option<FontStack> => FontStack, Static, FontStack;
    Common font_size: FontSize => FontSize, Both("FontSize"), FontSize;
    Common clip_children: bool => Plain, Static, Layout;
    Layout direction: FlexDirection => Plain, Both("Direction");
    Layout wrap: FlexWrap => Plain, Static;
    Layout justify: JustifyContent => Plain, Static;
    Layout align: AlignItems => Plain, Static;
    Layout padding: Padding => Plain, Responsive("Padding");
    Layout row_gap("gap"): Dimension => Dimension, Responsive("RowGap");
    Layout column_gap("gap"): Dimension => Dimension, Responsive("ColumnGap");
    Layout width: Dimension => Dimension, Both("Width");
    Layout min_width: Dimension => Dimension, Responsive("MinWidth");
    Layout max_width: Dimension => Dimension, Responsive("MaxWidth");
    Layout height: Dimension => Dimension, Both("Height");
    Layout min_height: Dimension => Dimension, Responsive("MinHeight");
    Layout max_height: Dimension => Dimension, Responsive("MaxHeight");
    Layout grow: Fixed => FlexFactor, Static;
    Layout shrink: Fixed => FlexFactor, Static;
    Layout position: Position => Plain, Static;
    Layout left: Dimension => Dimension, Both("Left");
    Layout top: Dimension => Dimension, Both("Top");
    Text paragraph("line_height"): ParagraphStyle => Plain, Both("Paragraph");
    Button normal_color: ThemedColor => Color, Both("ButtonNormalColor");
    Button pressed_color: ThemedColor => Color, Static;
    Button pressed_text_color: Option<ThemedColor> => OptionalColor, Static;
}

pub(crate) fn fields(name: &str) -> impl Iterator<Item = &'static Field> + '_ {
    FIELDS
        .iter()
        .filter(move |field| field.key == name || field.alias == Some(name))
}

pub(crate) fn common_names() -> impl Iterator<Item = &'static str> {
    FIELDS
        .iter()
        .filter(|field| matches!(field.target, Target::Common | Target::Layout))
        .flat_map(|field| core::iter::once(field.key).chain(field.alias))
}

pub(crate) fn property(widget: &str, name: &str, responsive: bool) -> Option<&'static str> {
    let field = FIELDS.iter().find(|field| field.key == name)?;
    if (field.target == Target::Text && !matches!(widget, "Text" | "Button"))
        || (field.target == Target::Button && widget != "Button")
    {
        return None;
    }
    match (field.binding, responsive) {
        (Binding::Both(name), _) | (Reactive(name), false) | (Responsive(name), true) => Some(name),
        _ => None,
    }
}

pub(crate) fn is_display_binding(property: &str) -> bool {
    FIELDS.iter().any(|field| match field.binding {
        Binding::Both(name) | Reactive(name) => name == property,
        _ => false,
    })
}

pub(crate) fn emit_schema() -> TokenStream {
    let mut common = Vec::new();
    let mut text = Vec::new();
    let mut button = Vec::new();
    let mut properties = Vec::new();
    for field in FIELDS {
        let key = format_ident!("{}", field.key);
        let ty = field.ty.parse::<TokenStream>().expect("style field type");
        match field.binding {
            Binding::Reactive(name) | Binding::Responsive(name) | Binding::Both(name) => {
                let property = format_ident!("{name}");
                properties.push(quote! { #property => #key });
            }
            Binding::None => {}
        }
        let change = match field.change {
            Change::Visual => quote!(ComponentChangeKind::Visual),
            Change::Layout => quote!(ComponentChangeKind::Layout),
            Change::FontStack => quote!(ComponentChangeKind::InheritedStyle(FONT_STACK)),
            Change::FontSize => quote!(ComponentChangeKind::InheritedStyle(FONT_SIZE)),
            Change::TextColor => quote!(ComponentChangeKind::InheritedStyle(TEXT_COLOR)),
        };
        match field.target {
            Target::Common => common.push(quote! { #key: #ty => #key [#change] }),
            Target::Layout => common.push(quote! { #key: #ty => layout.#key [#change] }),
            Target::Text => text.push(quote! { #key: #ty }),
            Target::Button => button.push(quote! { #key: #ty }),
        }
    }
    quote! {
        declare_styles! {
            common { #(#common,)* }
            text { #(#text,)* }
            button { #(#button,)* }
            properties { #(#properties,)* }
        }
    }
}

impl Field {
    pub(crate) fn layout_value(&self, value: &Expr, constant: bool) -> TokenStream {
        match self.convert {
            Convert::Dimension if constant => dimension(value),
            Convert::Dimension => quote! { ::mirui::types::Dimension::from(#value) },
            Convert::FlexFactor => quote! { ::mirui::types::Fixed::from_f32(#value) },
            _ => quote! { #value },
        }
    }

    fn preset_value(&self, name: &str, value: &Expr) -> TokenStream {
        match self.convert {
            Convert::Color => {
                quote! { ::core::convert::Into::into(#value) }
            }
            Convert::FontStack if name == "font" => quote! {
                Some(::mirui::render::font::FontStack::new(::core::convert::Into::into(#value)))
            },
            Convert::FontStack => quote! { Some(::core::convert::Into::into(#value)) },
            Convert::OptionalColor => optional_color(value),
            Convert::FontSize
                if matches!(
                    value,
                    Expr::Lit(ExprLit {
                        lit: Lit::Int(_),
                        ..
                    })
                ) =>
            {
                quote! { ::mirui::ui::FontSize::pixels(#value) }
            }
            Convert::FontSize => quote! { ::core::convert::Into::into(#value) },
            Convert::Fixed => fixed(value),
            _ => self.layout_value(value, true),
        }
    }
}

pub(crate) fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let attrs = syn::parse2::<DsAttrs>(quote! { (#input) })?;
    let mut common = Vec::new();
    let mut button = Vec::new();
    let mut paragraph = None;
    let mut line_height = None;
    let mut gap = None;
    let mut seen = std::collections::BTreeSet::new();
    for attr in attrs.attrs {
        let value = attr.value;
        let name = attr
            .name
            .ok_or_else(|| syn::Error::new(value.span(), "expected a named style field"))?;
        if attr.reactive {
            return Err(syn::Error::new(
                name.span(),
                "use a Rust expression for the style field",
            ));
        }
        let key = name.to_string();
        let mut resolved = fields(&key).peekable();
        if resolved.peek().is_none() {
            return Err(syn::Error::new(name.span(), "unknown style field"));
        }
        for field in resolved {
            let key_for_duplicates = if key == "line_height" {
                "line_height"
            } else {
                field.key
            };
            if !seen.insert(key_for_duplicates) {
                return Err(syn::Error::new(name.span(), "duplicate style field"));
            }
            let field_name = format_ident!("{}", field.key);
            let converted = field.preset_value(&key, &value);
            match field.target {
                Target::Text if key == "line_height" => line_height = Some(value.clone()),
                Target::Text => paragraph = Some(converted),
                Target::Button => button.push(quote! { #field_name: Some(#converted) }),
                Target::Layout if key == "gap" => {
                    let local = gap
                        .get_or_insert_with(|| {
                            let local = syn::Ident::new(
                                "__mirui_style_gap",
                                proc_macro2::Span::mixed_site(),
                            );
                            (local, converted.clone())
                        })
                        .0
                        .clone();
                    common.push(quote! { #field_name: Some(#local) });
                }
                _ => common.push(quote! { #field_name: Some(#converted) }),
            }
        }
    }
    if let Some(line_height) = line_height {
        let paragraph_value =
            paragraph.unwrap_or_else(|| quote! { ::mirui::ui::widgets::ParagraphStyle::default() });
        paragraph = Some(quote! { (#paragraph_value).with_line_height(#line_height) });
    }

    let common = quote! {
        ::mirui::ui::style::StyleFields { #(#common,)* ..::mirui::ui::style::StyleFields::EMPTY }
    };
    let preset = match (paragraph, button.is_empty()) {
        (None, true) => quote! { ::mirui::ui::StylePreset::from_common(#common) },
        (Some(paragraph), true) => {
            quote! { ::mirui::ui::StylePreset::from_text(#common, #paragraph) }
        }
        (None, false) => quote! {
            ::mirui::ui::StylePreset::from_button(#common, ::mirui::ui::style::ButtonStyleFields {
                #(#button,)* ..::mirui::ui::style::ButtonStyleFields::EMPTY
            })
        },
        (Some(paragraph), false) => {
            return Err(syn::Error::new(
                paragraph.span(),
                "a preset's widget-specific fields must belong to the same widget type",
            ));
        }
    };
    Ok(if let Some((name, value)) = gap {
        quote! {{ let #name = #value; #preset }}
    } else {
        preset
    })
}

fn optional_color(value: &Expr) -> TokenStream {
    if matches!(value, Expr::Path(path) if path.path.is_ident("None")) {
        quote! { None }
    } else {
        quote! { Some(::core::convert::Into::into(#value)) }
    }
}

fn dimension(value: &Expr) -> TokenStream {
    if let Expr::Lit(ExprLit {
        lit: Lit::Int(number),
        ..
    }) = value
        && matches!(number.suffix(), "" | "i32" | "u16")
    {
        let number = as_i32(number);
        quote! { ::mirui::types::Dimension::px(#number) }
    } else {
        quote! { ::mirui::types::Dimension::from(#value) }
    }
}

fn fixed(value: &Expr) -> TokenStream {
    match value {
        Expr::Lit(ExprLit {
            lit: Lit::Int(number),
            ..
        }) if matches!(number.suffix(), "" | "i32" | "u16") => {
            let number = as_i32(number);
            quote! { ::mirui::types::Fixed::from_int(#number) }
        }
        Expr::Lit(ExprLit {
            lit: Lit::Float(number),
            ..
        }) if matches!(number.suffix(), "" | "f32") => {
            quote! { ::mirui::types::Fixed::from_f32(#number) }
        }
        _ => quote! { ::mirui::types::Fixed::from(#value) },
    }
}

fn as_i32(number: &syn::LitInt) -> TokenStream {
    if number.suffix() == "u16" {
        quote! { #number as i32 }
    } else {
        quote! { #number }
    }
}
