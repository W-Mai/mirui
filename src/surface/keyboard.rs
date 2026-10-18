use crate::input::event::physical_key::physical_keys;

macro_rules! mappings {
    ($($key:ident, $sdl:ident, $linux:ident, $text:literal;)*) => {
        #[cfg(feature = "sdl")]
        pub(super) fn from_sdl(key: sdl2::keyboard::Scancode) -> Option<crate::input::event::input::PhysicalKey> {
            use crate::input::event::input::PhysicalKey;
            match key {
                $(sdl2::keyboard::Scancode::$sdl => Some(PhysicalKey::$key),)*
                _ => None,
            }
        }

        #[cfg(feature = "wgpu")]
        pub(super) fn from_winit(key: winit::keyboard::PhysicalKey) -> Option<crate::input::event::input::PhysicalKey> {
            use crate::input::event::input::PhysicalKey;
            match key {
                $(winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::$key) => Some(PhysicalKey::$key),)*
                _ => None,
            }
        }

        #[cfg(any(test, all(feature = "web-canvas", target_arch = "wasm32")))]
        pub(super) fn from_web(code: &str) -> Option<crate::input::event::input::PhysicalKey> {
            use crate::input::event::input::PhysicalKey;
            match code {
                "MetaLeft" => Some(PhysicalKey::SuperLeft),
                "MetaRight" => Some(PhysicalKey::SuperRight),
                $(stringify!($key) => Some(PhysicalKey::$key),)*
                _ => None,
            }
        }

        #[cfg(all(any(feature = "linux-fb", feature = "linux-drm"), target_os = "linux"))]
        pub(super) fn from_evdev(code: u16) -> Option<crate::input::event::input::PhysicalKey> {
            use crate::input::event::input::PhysicalKey;
            match evdev::KeyCode(code) {
                $(evdev::KeyCode::$linux => Some(PhysicalKey::$key),)*
                _ => None,
            }
        }

        #[cfg(test)]
        #[test]
        fn declared_positions_round_trip_without_allocating() {
            use crate::input::event::input::PhysicalKey;
            assert_eq!(crate::test_support::tracked_allocations(|| {
                $(
                    assert_eq!(from_web(stringify!($key)), Some(PhysicalKey::$key));
                    #[cfg(feature = "sdl")]
                    assert_eq!(from_sdl(sdl2::keyboard::Scancode::$sdl), Some(PhysicalKey::$key));
                    #[cfg(feature = "wgpu")]
                    assert_eq!(from_winit(winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::$key)), Some(PhysicalKey::$key));
                )*
                assert_eq!(from_web("Unidentified"), None);
                assert_eq!(from_web("é"), None);
            }), 0);
        }
    };
}

physical_keys!(mappings);
