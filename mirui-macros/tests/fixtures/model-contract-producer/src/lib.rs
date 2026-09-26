#![no_std]

pub mod data {
    use mirui::prelude::model;

    #[derive(Clone, Copy)]
    pub struct Change(u8);

    impl Change {
        pub const NONE: Self = Self(0);
        pub const VISUAL: Self = Self(1);

        pub fn contains(self, mask: Self) -> bool {
            self.0 & mask.0 == mask.0
        }
    }

    #[derive(Clone, Copy)]
    pub struct Pulse(pub u8);

    pub type PulseAlias = Pulse;

    #[cfg(feature = "extra")]
    type ExtraFlag = bool;

    #[model(change = Change, watch(visual = Change::VISUAL))]
    pub struct Meter<T, const N: usize>
    where
        T: Copy + Eq + 'static,
    {
        /// Current reading.
        #[observe]
        pub value: T,
        #[observe]
        pub(super) active: bool,
        #[cfg(feature = "extra")]
        #[observe]
        pub(super) feature_flag: ExtraFlag,
        #[cfg_attr(feature = "x", cfg(feature = "y"), doc = "Conditional reading.")]
        #[observe]
        pub(super) conditional_flag: bool,
        pub(super) pulses: [Option<Pulse>; N],
    }

    impl<T, const N: usize> Meter<T, N>
    where
        T: Copy + Eq + 'static,
    {
        pub fn new(value: T) -> Self {
            Self {
                value,
                active: true,
                #[cfg(feature = "extra")]
                feature_flag: false,
                #[cfg(any(not(feature = "x"), feature = "y"))]
                conditional_flag: true,
                pulses: [None; N],
            }
        }
    }
}

mod methods {
    use mirui::model;

    use super::data::{Change, PulseAlias};

    #[model]
    impl<T, const N: usize> super::data::Meter<T, N>
    where
        T: Copy + Eq + 'static,
    {
        /// Whether the meter accepts updates.
        #[observe]
        pub fn is_active(&self) -> bool {
            self.active
        }

        #[cfg_attr(feature = "x", cfg(feature = "y"), doc = "Conditional observer.")]
        #[observe]
        pub fn conditional_ready(&self) -> bool {
            self.conditional_flag
        }

        #[cfg(feature = "extra")]
        #[observe]
        pub fn has_feature(&self) -> bool {
            self.feature_flag
        }

        pub fn set_value(&mut self, value: T) -> Change {
            self.value = value;
            if let Some(first) = self.pulses.first_mut() {
                *first = Some(super::data::Pulse(1));
            }
            Change::VISUAL
        }

        #[cfg(not(feature = "extra"))]
        #[effects]
        fn take_plain_pulses(&mut self) -> [Option<PulseAlias>; N] {
            core::mem::replace(&mut self.pulses, [None; N])
        }

        #[cfg(feature = "extra")]
        #[effects]
        fn take_extra_pulses(&mut self) -> [Option<PulseAlias>; N] {
            core::mem::replace(&mut self.pulses, [None; N])
        }
    }
}

pub mod facade {
    pub use super::data::{Meter as PublicMeter, Pulse};
}

pub type SmallMeter = facade::PublicMeter<u8, 2>;
