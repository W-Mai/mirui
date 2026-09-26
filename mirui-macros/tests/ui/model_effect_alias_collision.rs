use mirui::model;

#[derive(Clone, Copy)]
struct Pulse;

type PulseAlias = Pulse;

#[model]
struct Counter;

#[model]
impl Counter {
    #[effects]
    fn first(&mut self) -> [Option<Pulse>; 1] {
        [None]
    }

    #[effects]
    fn second(&mut self) -> [Option<PulseAlias>; 1] {
        [None]
    }
}

fn main() {}
