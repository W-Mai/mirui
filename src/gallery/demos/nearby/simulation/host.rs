use alloc::rc::Rc;
use core::cell::Cell;

use crate::surface::{SurfaceSizeControl, SurfaceSizeError};

pub(super) const PANEL_WIDTH: u16 = 300;

#[derive(Clone, Copy)]
pub(crate) struct Visibility(pub bool);

#[derive(Clone, Copy)]
struct Layout {
    main: (u16, u16),
    docked: bool,
    wanted: bool,
    error: Option<SurfaceSizeError>,
}

/// Keeps demo dimensions separate from the host's total content dimensions.
#[derive(Clone)]
pub(crate) struct InspectorHost {
    size: SurfaceSizeControl,
    layout: Rc<Cell<Layout>>,
}

impl InspectorHost {
    pub fn new(size: SurfaceSizeControl) -> Self {
        let main = size.state_untracked().actual;
        Self {
            size,
            layout: Rc::new(Cell::new(Layout {
                main,
                docked: false,
                wanted: false,
                error: None,
            })),
        }
    }

    fn refresh(&self) -> Layout {
        let state = self.size.state();
        let mut layout = self.layout.get();
        if !state.available {
            layout.docked = false;
            layout.wanted = false;
        } else if let Some(error) = state.error {
            if layout.error != Some(error) {
                crate::warn!("simulation panel size request rejected: {:?}", error);
                layout.error = Some(error);
                layout.wanted = layout.docked;
                self.layout.set(layout);
                let extra = if layout.docked { PANEL_WIDTH } else { 0 };
                if let Some(width) = layout.main.0.checked_add(extra) {
                    let _ = self.size.request_size(width, layout.main.1);
                }
            }
        } else if state.requested.is_none() {
            layout.error = None;
            let extra = if layout.wanted { PANEL_WIDTH } else { 0 };
            let expected = layout
                .main
                .0
                .checked_add(extra)
                .map(|width| (width, layout.main.1));
            if layout.docked != layout.wanted && expected == Some(state.actual) {
                layout.docked = layout.wanted;
            }
            if layout.docked == layout.wanted {
                let extra = if layout.docked { PANEL_WIDTH } else { 0 };
                layout.main = (state.actual.0.saturating_sub(extra).max(1), state.actual.1);
            }
        }
        self.layout.set(layout);
        layout
    }

    pub fn request(&self, visible: bool) {
        let mut layout = self.refresh();
        layout.wanted = visible;
        self.layout.set(layout);
        let extra = if visible { PANEL_WIDTH } else { 0 };
        let Some(width) = layout.main.0.checked_add(extra) else {
            crate::warn!("simulation panel content size exceeds the surface range");
            return;
        };
        if let Err(error) = self.size.request_size(width, layout.main.1) {
            crate::warn!("simulation panel size request failed: {:?}", error);
        }
    }

    pub fn visible(&self) -> bool {
        self.refresh().docked
    }
    pub fn main_size(&self) -> (u16, u16) {
        self.refresh().main
    }
}
