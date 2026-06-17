//! Input pipeline. `event` carries the raw stream + dispatch + gesture
//! recognition + scroll / focus / hit-test. `feedback` paints overlay
//! visualisations of the input the framework just routed.

pub mod event;
pub mod feedback;
