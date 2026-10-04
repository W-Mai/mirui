use mirui::system;

const ROOT_OFFSET: u32 = 2;

mod nested {
    use super::system;

    const LOCAL_OFFSET: u32 = 3;

    fn local_offset() -> u32 {
        5
    }

    #[system]
    #[doc = "A relative-path system."]
    #[allow(clippy::let_and_return)]
    #[cfg_attr(all(), cfg(all()), doc = "A typed system callback.", inline)]
    pub fn relative_paths() {
        let value = super::ROOT_OFFSET + self::LOCAL_OFFSET + self::local_offset();
        assert_eq!(value, 10);
    }

    #[system]
    #[cfg(any())]
    pub fn disabled() {
        unresolved_when_enabled();
    }
}

fn main() {
    let _ = nested::relative_paths::system();
}
