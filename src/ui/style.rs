//! Reusable, typed declarations for widget styles.

use crate::ecs::{Entity, World};
use crate::render::font::FontStack;
use crate::types::{Dimension, Fixed};
use crate::ui::Style;
use crate::ui::layout::{AlignItems, FlexDirection, FlexWrap, JustifyContent, Padding, Position};
use crate::ui::theme::ThemedColor;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

macro_rules! declare_styles {
    (
        common { $( $key:ident: $ty:ty => $($target:ident).+,)* }
        text { $($text:ident: $text_ty:ty,)* }
        button { $($button:ident: $button_ty:ty,)* }
    ) => {
        /// Common field identifiers shared by style declaration and construction.
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #[repr(u8)]
        #[derive(Clone, Copy)]
        pub enum StyleField {
            $($key,)+
            $($text,)*
            $($button,)*
        }

        #[doc(hidden)]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct StyleFields {
            $(pub $key: Option<$ty>,)+
        }

        impl StyleFields {
            pub const EMPTY: Self = Self { $($key: None,)+ };

            fn from_style(style: Style) -> Self {
                Self { $($key: Some(style.$($target).+),)+ }
            }

            fn apply(self, style: &mut Style, explicit: u64) -> u64 {
                let mut applied = 0;
                $(if explicit & StyleField::$key.bit() == 0 && let Some(value) = self.$key {
                    style.$($target).+ = value;
                    applied |= StyleField::$key.bit();
                })+
                applied
            }
        }

        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct ButtonStyleFields {
            $(pub $button: Option<$button_ty>,)*
        }

        impl ButtonStyleFields {
            pub const EMPTY: Self = Self { $($button: None,)* };

            fn apply(self, button: &mut Button, explicit: u64) {
                $(if explicit & StyleField::$button.bit() == 0 && let Some(value) = self.$button {
                    button.$button = value;
                })*
            }
        }
    };
}

mirui_macros::__style_fields!();

impl StyleField {
    pub const fn bit(self) -> u64 {
        1 << self as u8
    }
}

#[derive(Clone, Copy)]
pub(super) struct ExplicitButtonLayout(pub(super) u64);

impl crate::ecs::Component for ExplicitButtonLayout {}

#[doc(hidden)]
pub fn mark_button_layout(world: &mut World, entity: Entity, fields: u64) {
    let layout = fields
        & (StyleField::padding.bit()
            | StyleField::min_height.bit()
            | StyleField::justify.bit()
            | StyleField::align.bit());
    if layout != 0 {
        world.insert(entity, ExplicitButtonLayout(layout));
    }
}

/// An independently reusable set of declared style fields.
///
/// Use [`crate::style!`] to construct a preset. The default type parameter
/// allows the preset to be used on any widget.
pub struct StylePreset<W: StyleTarget = ()> {
    common: StyleFields,
    specific: W::Fields,
}

impl<W: StyleTarget> Clone for StylePreset<W> {
    fn clone(&self) -> Self {
        Self {
            common: self.common.clone(),
            specific: self.specific.clone(),
        }
    }
}

#[doc(hidden)]
pub trait StyleTarget {
    type Fields: Clone;
}

impl StyleTarget for () {
    type Fields = ();
}

impl StyleTarget for Text {
    type Fields = Option<ParagraphStyle>;
}

impl StyleTarget for Button {
    type Fields = ButtonStyleFields;
}

impl StylePreset {
    #[doc(hidden)]
    pub const fn from_common(common: StyleFields) -> Self {
        Self {
            common,
            specific: (),
        }
    }
}

impl StylePreset<Text> {
    #[doc(hidden)]
    pub const fn from_text(common: StyleFields, paragraph: ParagraphStyle) -> Self {
        Self {
            common,
            specific: Some(paragraph),
        }
    }
}

impl StylePreset<Button> {
    #[doc(hidden)]
    pub const fn from_button(common: StyleFields, button: ButtonStyleFields) -> Self {
        Self {
            common,
            specific: button,
        }
    }
}

/// A style accepted by a widget's builder.
#[doc(hidden)]
pub trait StyleInput<W> {
    const HAS_WIDGET_FIELDS: bool = false;
    const COMPLETE: bool = false;

    fn apply_widget(&mut self, _widget: &mut W, _explicit: u64) {}
    fn apply_style(self, style: &mut Style, explicit: u64) -> u64;
}

impl<W> StyleInput<W> for StylePreset {
    fn apply_style(self, style: &mut Style, explicit: u64) -> u64 {
        self.common.apply(style, explicit)
    }
}

impl<W> StyleInput<W> for Style {
    const COMPLETE: bool = true;

    fn apply_style(self, style: &mut Style, explicit: u64) -> u64 {
        if explicit == 0 {
            *style = self;
            0
        } else {
            StyleFields::from_style(self).apply(style, explicit)
        }
    }
}

impl StyleInput<Text> for StylePreset<Text> {
    const HAS_WIDGET_FIELDS: bool = true;

    fn apply_widget(&mut self, text: &mut Text, explicit: u64) {
        if explicit & StyleField::paragraph.bit() == 0
            && let Some(paragraph) = self.specific.take()
        {
            text.set_paragraph(paragraph);
        }
    }

    fn apply_style(self, style: &mut Style, explicit: u64) -> u64 {
        self.common.apply(style, explicit)
    }
}

impl StyleInput<Button> for StylePreset<Button> {
    const HAS_WIDGET_FIELDS: bool = true;

    fn apply_widget(&mut self, button: &mut Button, explicit: u64) {
        self.specific.apply(button, explicit);
    }

    fn apply_style(self, style: &mut Style, explicit: u64) -> u64 {
        self.common.apply(style, explicit)
    }
}

#[doc(hidden)]
pub fn apply_preset<W: 'static, S: StyleInput<W>>(
    world: &mut World,
    entity: Entity,
    mut preset: S,
    explicit: u64,
) -> u64 {
    if S::HAS_WIDGET_FIELDS
        && let Some(widget) = world.get_mut::<W>(entity)
    {
        preset.apply_widget(widget, explicit);
    }
    let Some(style) = world.get_mut::<Style>(entity) else {
        return explicit;
    };
    explicit | preset.apply_style(style, explicit)
}
