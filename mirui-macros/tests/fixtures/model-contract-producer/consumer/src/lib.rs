#![no_std]

use mirui::core::model::{BindType, Model};
use model_contract_producer::SmallMeter;

pub fn read_reexported_model(handle: &<SmallMeter as Model>::Handle) -> u8 {
    handle.value()
}

pub fn use_shared_alias(handle: <SmallMeter as BindType>::Shared) {
    handle.set_value(7);
    let _ = handle.visual_revision();
}

#[cfg(feature = "extra")]
pub fn read_conditional_observer(handle: &<SmallMeter as Model>::Handle) -> bool {
    handle.has_feature()
}

#[mirui::model]
pub struct LocalCounter {
    value: u8,
}

#[mirui::model]
impl LocalCounter {
    pub fn increment(&mut self) {
        self.value += 1;
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::prelude::App;
    use model_contract_producer::facade::Pulse;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn reexported_generic_model_works_across_crate_boundaries() {
        let mut app = App::headless(16, 16);
        let meter = app.add_model(SmallMeter::new(3));
        let shared: <SmallMeter as BindType>::Shared = meter.clone();
        assert_eq!(read_reexported_model(&meter), 3);
        assert!(shared.is_active());
        #[cfg(feature = "extra")]
        assert!(!read_conditional_observer(&meter));
        assert_eq!(meter.visual_revision(), 0);

        let pulses = Rc::new(Cell::new(0));
        let recorded = Rc::clone(&pulses);
        app.on_effect(&meter, move |pulse: Pulse| {
            assert_eq!(pulse.0, 1);
            recorded.set(recorded.get() + 1);
        })
        .unwrap();
        shared.set_value(7);
        assert_eq!(meter.value(), 7);
        assert_eq!(meter.visual_revision(), 1);
        assert_eq!(pulses.get(), 1);

        let local = app.add_model(LocalCounter { value: 0 });
        local.increment();
        assert_eq!(
            mirui::core::model::ModelHandle::read(&local, |data| data.value),
            1
        );
    }
}
