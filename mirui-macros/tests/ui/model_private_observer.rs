mod data {
    use mirui::model;

    #[model]
    pub struct Counter {
        #[observe]
        value: u8,
    }

    #[model]
    impl Counter {}
}

fn read(handle: &data::CounterHandle) -> u8 {
    handle.value()
}

fn main() {}
