macro_rules! physical_keys {
    ($consumer:ident) => {
        $consumer! {
            KeyA, A, KEY_A, true;
            KeyB, B, KEY_B, true;
            KeyC, C, KEY_C, true;
            KeyD, D, KEY_D, true;
            KeyE, E, KEY_E, true;
            KeyF, F, KEY_F, true;
            KeyG, G, KEY_G, true;
            KeyH, H, KEY_H, true;
            KeyI, I, KEY_I, true;
            KeyJ, J, KEY_J, true;
            KeyK, K, KEY_K, true;
            KeyL, L, KEY_L, true;
            KeyM, M, KEY_M, true;
            KeyN, N, KEY_N, true;
            KeyO, O, KEY_O, true;
            KeyP, P, KEY_P, true;
            KeyQ, Q, KEY_Q, true;
            KeyR, R, KEY_R, true;
            KeyS, S, KEY_S, true;
            KeyT, T, KEY_T, true;
            KeyU, U, KEY_U, true;
            KeyV, V, KEY_V, true;
            KeyW, W, KEY_W, true;
            KeyX, X, KEY_X, true;
            KeyY, Y, KEY_Y, true;
            KeyZ, Z, KEY_Z, true;
            Digit0, Num0, KEY_0, true;
            Digit1, Num1, KEY_1, true;
            Digit2, Num2, KEY_2, true;
            Digit3, Num3, KEY_3, true;
            Digit4, Num4, KEY_4, true;
            Digit5, Num5, KEY_5, true;
            Digit6, Num6, KEY_6, true;
            Digit7, Num7, KEY_7, true;
            Digit8, Num8, KEY_8, true;
            Digit9, Num9, KEY_9, true;
            Backquote, Grave, KEY_GRAVE, true;
            Backslash, Backslash, KEY_BACKSLASH, true;
            BracketLeft, LeftBracket, KEY_LEFTBRACE, true;
            BracketRight, RightBracket, KEY_RIGHTBRACE, true;
            Comma, Comma, KEY_COMMA, true;
            Equal, Equals, KEY_EQUAL, true;
            Minus, Minus, KEY_MINUS, true;
            Period, Period, KEY_DOT, true;
            Quote, Apostrophe, KEY_APOSTROPHE, true;
            Semicolon, Semicolon, KEY_SEMICOLON, true;
            Slash, Slash, KEY_SLASH, true;
            Space, Space, KEY_SPACE, true;
            IntlBackslash, NonUsBackslash, KEY_102ND, true;
            IntlRo, International1, KEY_RO, true;
            IntlYen, International3, KEY_YEN, true;
            Convert, International4, KEY_HENKAN, true;
            NonConvert, International5, KEY_MUHENKAN, true;
            KanaMode, International2, KEY_KATAKANAHIRAGANA, true;
            Lang1, Lang1, KEY_HANGEUL, true;
            Lang2, Lang2, KEY_HANJA, true;
            Lang3, Lang3, KEY_KATAKANA, true;
            Lang4, Lang4, KEY_HIRAGANA, true;
            Lang5, Lang5, KEY_ZENKAKUHANKAKU, true;
            Numpad0, Kp0, KEY_KP0, true;
            Numpad1, Kp1, KEY_KP1, true;
            Numpad2, Kp2, KEY_KP2, true;
            Numpad3, Kp3, KEY_KP3, true;
            Numpad4, Kp4, KEY_KP4, true;
            Numpad5, Kp5, KEY_KP5, true;
            Numpad6, Kp6, KEY_KP6, true;
            Numpad7, Kp7, KEY_KP7, true;
            Numpad8, Kp8, KEY_KP8, true;
            Numpad9, Kp9, KEY_KP9, true;
            NumpadAdd, KpPlus, KEY_KPPLUS, true;
            NumpadSubtract, KpMinus, KEY_KPMINUS, true;
            NumpadMultiply, KpMultiply, KEY_KPASTERISK, true;
            NumpadDivide, KpDivide, KEY_KPSLASH, true;
            NumpadDecimal, KpPeriod, KEY_KPDOT, true;
            NumpadComma, KpComma, KEY_KPCOMMA, true;
            NumpadEqual, KpEquals, KEY_KPEQUAL, true;
            NumpadEnter, KpEnter, KEY_KPENTER, false;
            NumLock, NumLockClear, KEY_NUMLOCK, false;
            Enter, Return, KEY_ENTER, false;
            Escape, Escape, KEY_ESC, false;
            Tab, Tab, KEY_TAB, false;
            Backspace, Backspace, KEY_BACKSPACE, false;
            Delete, Delete, KEY_DELETE, false;
            Insert, Insert, KEY_INSERT, false;
            Home, Home, KEY_HOME, false;
            End, End, KEY_END, false;
            PageUp, PageUp, KEY_PAGEUP, false;
            PageDown, PageDown, KEY_PAGEDOWN, false;
            ArrowLeft, Left, KEY_LEFT, false;
            ArrowRight, Right, KEY_RIGHT, false;
            ArrowUp, Up, KEY_UP, false;
            ArrowDown, Down, KEY_DOWN, false;
            CapsLock, CapsLock, KEY_CAPSLOCK, false;
            ShiftLeft, LShift, KEY_LEFTSHIFT, false;
            ShiftRight, RShift, KEY_RIGHTSHIFT, false;
            ControlLeft, LCtrl, KEY_LEFTCTRL, false;
            ControlRight, RCtrl, KEY_RIGHTCTRL, false;
            AltLeft, LAlt, KEY_LEFTALT, false;
            AltRight, RAlt, KEY_RIGHTALT, false;
            SuperLeft, LGui, KEY_LEFTMETA, false;
            SuperRight, RGui, KEY_RIGHTMETA, false;
            ContextMenu, Application, KEY_COMPOSE, false;
            PrintScreen, PrintScreen, KEY_SYSRQ, false;
            ScrollLock, ScrollLock, KEY_SCROLLLOCK, false;
            Pause, Pause, KEY_PAUSE, false;
            F1, F1, KEY_F1, false;
            F2, F2, KEY_F2, false;
            F3, F3, KEY_F3, false;
            F4, F4, KEY_F4, false;
            F5, F5, KEY_F5, false;
            F6, F6, KEY_F6, false;
            F7, F7, KEY_F7, false;
            F8, F8, KEY_F8, false;
            F9, F9, KEY_F9, false;
            F10, F10, KEY_F10, false;
            F11, F11, KEY_F11, false;
            F12, F12, KEY_F12, false;
            F13, F13, KEY_F13, false;
            F14, F14, KEY_F14, false;
            F15, F15, KEY_F15, false;
            F16, F16, KEY_F16, false;
            F17, F17, KEY_F17, false;
            F18, F18, KEY_F18, false;
            F19, F19, KEY_F19, false;
            F20, F20, KEY_F20, false;
            F21, F21, KEY_F21, false;
            F22, F22, KEY_F22, false;
            F23, F23, KEY_F23, false;
            F24, F24, KEY_F24, false;
        }
    };
}

pub(crate) use physical_keys;

macro_rules! declare {
    ($($key:ident, $sdl:ident, $linux:ident, $text:literal;)*) => {
        /// Keyboard positions, independent of the active text layout. Letter names describe US key positions, not committed characters.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum PhysicalKey { $($key,)* }

        impl PhysicalKey {
            pub(crate) fn is_text_entry(self) -> bool {
                match self { $(Self::$key => $text,)* }
            }
        }
    };
}

physical_keys!(declare);
