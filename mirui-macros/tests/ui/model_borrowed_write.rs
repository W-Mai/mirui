use mirui::model;

struct Counter(u8);

#[model]
impl Counter {
    fn value_mut(&mut self) -> &mut u8 {
        &mut self.0
    }
}

fn main() {}
