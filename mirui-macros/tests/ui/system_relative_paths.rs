use mirui::system;

const ROOT_OFFSET: u32 = 2;

#[mirui::model]
struct Counter {
    value: u32,
}

#[mirui::model]
impl Counter {
    fn add(&mut self, value: u32) {
        self.value += value;
    }
}

#[mirui::component]
struct Position(u32);

struct Clock(u32);
struct Metrics;

mod nested {
    use super::system;

    const LOCAL_OFFSET: u32 = 3;

    #[mirui::component]
    struct Active;

    #[mirui::component]
    struct Hidden;

    fn local_offset() -> u32 {
        5
    }

    #[system(expect = self::Active)]
    #[doc = "A relative-path system."]
    #[allow(clippy::let_and_return)]
    #[cfg_attr(all(), cfg(all()), doc = "A typed system callback.", inline)]
    pub fn relative_paths(
        counter: model!(super::Counter),
        clock: res!(super::Clock),
        metrics: res!(optional super::Metrics),
        rows: query!(mut super::Position; with self::Active; without self::Hidden;),
    ) {
        let value = super::ROOT_OFFSET + self::LOCAL_OFFSET + self::local_offset();
        assert_eq!(value, 10);
        assert!(metrics.is_none());
        for position in rows {
            position.0 += clock.0;
            counter.add(position.0);
        }
    }

    #[system]
    #[cfg(any())]
    pub fn disabled() {
        unresolved_when_enabled();
    }
}

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 }).finish();
    let _ = nested::relative_paths::system(counter);
}
