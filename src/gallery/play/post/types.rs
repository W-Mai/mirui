use super::route::{PostRoute, position_on_route};
use crate::types::{Fixed, Point};

pub(crate) const MAX_PARCELS: usize = 3;
pub(crate) const MAX_EVENTS: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostParcel {
    pub(super) id: u8,
    pub(super) target: u8,
    pub(super) route: PostRoute,
    pub(super) distance: Fixed,
    pub(super) decisions: [u8; 2],
    pub(super) decision_count: u8,
}

impl PostParcel {
    pub(super) const EMPTY: Self = Self {
        id: 0,
        target: 0,
        route: PostRoute::Entry,
        distance: Fixed::ZERO,
        decisions: [0; 2],
        decision_count: 0,
    };

    pub(super) const fn new(id: u8, target: u8) -> Self {
        Self {
            id,
            target,
            ..Self::EMPTY
        }
    }

    pub(crate) const fn target(self) -> u8 {
        self.target
    }

    #[cfg(test)]
    pub(crate) const fn route(self) -> PostRoute {
        self.route
    }

    #[cfg(test)]
    pub(crate) const fn distance(self) -> Fixed {
        self.distance
    }

    #[cfg(test)]
    pub(crate) const fn decision_count(self) -> u8 {
        self.decision_count
    }

    pub(crate) fn position(self) -> Point {
        position_on_route(self.route, self.distance)
    }

    pub(super) fn lock_decision(&mut self, decision: u8) {
        if usize::from(self.decision_count) < self.decisions.len() {
            self.decisions[usize::from(self.decision_count)] = decision;
            self.decision_count += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeliveryEvent {
    pub(crate) parcel_id: u8,
    pub(crate) target: u8,
    pub(crate) station: u8,
    pub(crate) correct: bool,
}

impl DeliveryEvent {
    pub(super) const EMPTY: Self = Self {
        parcel_id: 0,
        target: 0,
        station: 0,
        correct: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostModal {
    None,
    Manifests,
    Summary,
    Reset,
}
