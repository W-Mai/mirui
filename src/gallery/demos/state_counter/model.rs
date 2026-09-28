#[derive(Default)]
#[crate::model]
pub struct CounterModel {
    #[observe]
    pub(super) count: i32,
}

#[crate::model]
impl CounterModel {
    pub fn set_count(&mut self, count: i32) {
        self.count = count;
    }

    pub fn decrement(&mut self) {
        self.count -= 1;
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }
}
