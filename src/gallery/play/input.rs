#[derive(Clone, Copy, Debug)]
pub(crate) struct DragTransaction<T: Copy> {
    original: T,
    current: T,
}

impl<T: Copy> DragTransaction<T> {
    pub(crate) const fn begin(value: T) -> Self {
        Self {
            original: value,
            current: value,
        }
    }

    pub(crate) fn update(&mut self, value: T) -> T {
        self.current = value;
        value
    }

    pub(crate) const fn commit(self) -> T {
        self.current
    }

    pub(crate) const fn cancel(self) -> T {
        self.original
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_keeps_latest_value() {
        let mut drag = DragTransaction::begin(4);
        drag.update(7);
        assert_eq!(drag.commit(), 7);
    }

    #[test]
    fn cancel_restores_original_value() {
        let mut drag = DragTransaction::begin(4);
        drag.update(7);
        assert_eq!(drag.cancel(), 4);
    }
}
