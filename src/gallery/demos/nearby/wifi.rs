//! Simulated ordinary Wi-Fi settings. No host network access is performed.

use super::text_input::Edit;
use crate::anim::{Tween, TweenConfig, ease};
use crate::types::Fixed;

pub(super) fn loading_span(now: u64) -> (Fixed, Fixed) {
    const TRAVEL_MS: u64 = 1500;
    const STRETCH_MS: u16 = 350;
    let elapsed = (now % TRAVEL_MS) as u16;
    let sample = |offset: u16| {
        let mut tween = Tween::preset(
            Fixed::ZERO,
            Fixed::ONE,
            TweenConfig::new(TRAVEL_MS as u16 - STRETCH_MS, &ease::IN_OUT_CUBIC),
        );
        tween.tick(elapsed.saturating_sub(offset));
        tween.value()
    };
    let tail = sample(STRETCH_MS) * Fixed::from_ratio(4, 5);
    let head = sample(0) * Fixed::from_ratio(4, 5) + Fixed::from_ratio(1, 5);
    if now / TRAVEL_MS % 2 == 0 {
        (tail, head)
    } else {
        (Fixed::ONE - head, Fixed::ONE - tail)
    }
}
use alloc::string::{String, ToString};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    List,
    Password,
    Hidden,
    Security,
    Connecting,
    Cancelling,
    Restoring,
    Success,
    Info,
    Details,
    Forget,
    Off,
    Empty,
    Busy,
    Failed,
}

impl Page {
    pub const fn keyboard(self) -> bool {
        matches!(self, Self::Password | Self::Hidden)
    }
    pub const fn pending(self) -> bool {
        matches!(self, Self::Connecting | Self::Cancelling | Self::Restoring)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    Open,
    Psk,
    Enterprise,
}
impl Security {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "开放",
            Self::Psk => "WPA / WPA2",
            Self::Enterprise => "企业认证",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Refresh,
    Auxiliary,
    Start,
    Confirm,
    Back,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Normal,
    WrongPassword,
    ConnectionFailure,
    RestoreFailure,
    RadioOff,
    Empty,
    Busy,
}
impl Scenario {
    pub const ALL: [Self; 7] = [
        Self::Normal,
        Self::WrongPassword,
        Self::ConnectionFailure,
        Self::RestoreFailure,
        Self::RadioOff,
        Self::Empty,
        Self::Busy,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Wi-Fi: success",
            Self::WrongPassword => "Wi-Fi: wrong password",
            Self::ConnectionFailure => "Wi-Fi: failure",
            Self::RestoreFailure => "Wi-Fi: restore fails",
            Self::RadioOff => "Wi-Fi: radio off",
            Self::Empty => "Wi-Fi: empty",
            Self::Busy => "Wi-Fi: room busy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: &'static str,
    pub security: Security,
    pub signal: u8,
    pub saved: bool,
    pub connected: bool,
}

const NETWORKS: [Network; 4] = [
    Network {
        ssid: "Camp Wi-Fi",
        security: Security::Psk,
        signal: 82,
        saved: true,
        connected: true,
    },
    Network {
        ssid: "Guest's network",
        security: Security::Open,
        signal: 60,
        saved: false,
        connected: false,
    },
    Network {
        ssid: "Pocket Wi-Fi",
        security: Security::Psk,
        signal: 46,
        saved: false,
        connected: false,
    },
    Network {
        ssid: "Office",
        security: Security::Enterprise,
        signal: 28,
        saved: false,
        connected: false,
    },
];

pub(super) enum Response {
    Stay,
    Close,
    Edit(Edit),
}

pub(super) struct State {
    pub page: Page,
    pub selected: usize,
    pub stage: u8,
    pub scanning: bool,
    pub scenario: Scenario,
    pub notice: &'static str,
    pub editor: Editor,
    ssid: String,
    hidden_name: String,
    hidden_security: Security,
    target_security: Security,
    connected: Option<usize>,
    saved: u8,
    target: Option<usize>,
    list_selection: Option<usize>,
    info_entry: bool,
    radio: bool,
    count: usize,
    elapsed: u16,
    pub visible_rows: usize,
    pub pulse: u16,
    frozen: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            page: Page::List,
            selected: 0,
            stage: 0,
            scanning: false,
            scenario: Scenario::Normal,
            notice: "",
            editor: Editor::default(),
            ssid: String::with_capacity(32),
            hidden_name: String::with_capacity(32),
            hidden_security: Security::Psk,
            target_security: Security::Psk,
            connected: Some(0),
            saved: 1,
            target: None,
            list_selection: Some(0),
            info_entry: false,
            radio: true,
            count: NETWORKS.len(),
            elapsed: 0,
            visible_rows: 1,
            pulse: 0,
            frozen: false,
        }
    }
}

impl State {
    pub fn loading_span(&self, phase: u16) -> (Fixed, Fixed) {
        loading_span(if self.frozen { 800 } else { u64::from(phase) })
    }
    pub fn close(&mut self) {
        self.editor.clear();
        self.scanning = false;
        self.notice = "";
        self.target = None;
        self.elapsed = 0;
    }
    pub fn open(&mut self, info: bool) {
        self.editor.clear();
        self.target = None;
        self.ssid.clear();
        self.notice = "";
        self.selected = 0;
        self.list_selection = self.slot(0);
        self.info_entry = info;
        self.frozen = false;
        self.elapsed = 0;
        self.scanning = !info;
        self.pulse = 0;
        self.page = if self.scenario == Scenario::Busy {
            Page::Busy
        } else if info {
            Page::Info
        } else if !self.radio {
            Page::Off
        } else if self.count == 0 {
            Page::Empty
        } else {
            Page::List
        };
    }
    pub fn set_scenario(&mut self, scenario: Scenario) {
        self.scenario = scenario;
        self.radio = scenario != Scenario::RadioOff;
        self.count = if scenario == Scenario::Empty {
            0
        } else {
            NETWORKS.len()
        };
        self.open(false);
    }
    pub fn fixture(&mut self, page: Page) {
        self.open(false);
        self.page = page;
        self.stage = 1;
        self.scanning = page == Page::List;
        self.pulse = if self.scanning { 2 } else { 0 };
        self.count = 2;
        self.target = Some(0);
        self.ssid.push_str(NETWORKS[0].ssid);
        self.frozen = true;
        self.notice = if page == Page::Failed {
            "未能连接，请检查密码或信号"
        } else {
            ""
        };
    }
    fn slot(&self, index: usize) -> Option<usize> {
        if index >= self.count {
            return None;
        }
        Some(match self.connected.filter(|&key| key < self.count) {
            Some(key) if index == 0 => key,
            Some(key) if index <= key => index - 1,
            _ => index,
        })
    }
    fn index(&self, key: usize) -> Option<usize> {
        (0..self.count).find(|&index| self.slot(index) == Some(key))
    }
    pub fn row(&self, index: usize) -> Option<Network> {
        let index = self.slot(index)?;
        let mut row = if index == NETWORKS.len() {
            Network {
                ssid: "",
                security: self.hidden_security,
                signal: 72,
                saved: false,
                connected: false,
            }
        } else {
            *NETWORKS.get(index)?
        };
        row.connected = self.connected == Some(index);
        row.saved = self.saved & (1 << index) != 0;
        Some(row)
    }
    pub fn name(&self, index: usize) -> String {
        if self.slot(index) == Some(NETWORKS.len()) {
            self.hidden_name.clone()
        } else {
            self.row(index)
                .map_or_else(String::new, |row| row.ssid.into())
        }
    }
    pub fn text(&self, index: usize) -> String {
        match index {
            0 => self.ssid.clone(),
            1 => {
                if self.editor.error.is_empty() {
                    self.notice.into()
                } else {
                    self.editor.error.into()
                }
            }
            _ => String::new(),
        }
    }
    pub fn title(&self) -> String {
        match self.page {
            Page::Password | Page::Security | Page::Connecting | Page::Details => self.ssid.clone(),
            Page::Info => "网络信息".into(),
            Page::Hidden => "添加网络".into(),
            Page::Success => "连接完成".into(),
            Page::Forget => "忘记网络".into(),
            Page::Cancelling => "正在取消".into(),
            Page::Restoring => "正在恢复网络".into(),
            Page::Busy => "当前网络".into(),
            _ => "Wi-Fi 设置".into(),
        }
    }
    pub fn fact(&self, index: usize) -> String {
        let connected_row = self.connected.and_then(|key| self.index(key));
        let Some(row) = connected_row.and_then(|index| self.row(index)) else {
            return if index == 0 {
                "尚未连接".into()
            } else {
                String::new()
            };
        };
        match index {
            0 => connected_row.map_or_else(String::new, |index| self.name(index)),
            1 => "192.168.1.14/24".into(),
            2 => "192.168.1.1".into(),
            3 => alloc::format!("{}%", row.signal),
            _ => row.security.label().into(),
        }
    }
    pub fn window(&self) -> core::ops::Range<usize> {
        let count = self.count + 1;
        let rows = self.visible_rows.clamp(1, 8).min(count);
        let first = self.selected.saturating_sub(rows - 1).min(count - rows);
        first..first + rows
    }
    fn list(&mut self) {
        self.page = if self.count == 0 {
            Page::Empty
        } else {
            Page::List
        };
        self.selected = self
            .list_selection
            .and_then(|key| self.index(key))
            .unwrap_or(self.count);
        self.editor.clear();
        self.elapsed = 0;
    }
    fn connect(&mut self) {
        self.page = Page::Connecting;
        self.stage = 0;
        self.elapsed = 0;
        self.notice = "";
    }
    pub fn tick(&mut self, delta: u16) -> bool {
        if self.frozen || (!self.page.pending() && !self.scanning) {
            return false;
        }
        let previous = (self.page, self.stage, self.scanning, self.pulse);
        self.elapsed = self.elapsed.saturating_add(delta);
        self.pulse = if self.scanning {
            (self.elapsed / 125) % 4
        } else {
            0
        };
        if self.scanning && self.elapsed >= 700 {
            self.scanning = false;
            self.pulse = 0;
        }
        if self.page.pending() && self.elapsed >= 700 {
            self.elapsed -= 700;
            match self.page {
                Page::Cancelling => self.page = Page::Restoring,
                Page::Restoring => {
                    if self.scenario == Scenario::RestoreFailure {
                        self.page = Page::Failed;
                        self.notice = "恢复网络未完成，请重试";
                        self.selected = 0;
                    } else {
                        self.list();
                        self.notice = "已取消连接";
                    }
                }
                Page::Connecting if self.stage < 1 => self.stage += 1,
                Page::Connecting => match self.scenario {
                    Scenario::WrongPassword => {
                        self.page = Page::Password;
                        self.notice = "密码错误，请重新输入";
                        self.editor.visible = false;
                    }
                    Scenario::ConnectionFailure => {
                        self.page = Page::Failed;
                        self.notice = "未能连接，请检查密码或信号";
                        self.selected = 0;
                    }
                    _ => {
                        self.connected = self.target;
                        if let Some(index) = self.target {
                            self.saved |= 1 << index;
                            if index == NETWORKS.len() {
                                self.hidden_name.clear();
                                self.hidden_name.push_str(&self.ssid);
                                self.hidden_security = self.target_security;
                                self.count = NETWORKS.len() + 1;
                                self.list_selection = Some(index);
                            }
                        }
                        self.page = Page::Success;
                        self.stage = 2;
                        self.selected = 0;
                        self.editor.clear();
                    }
                },
                _ => {}
            }
        }
        previous != (self.page, self.stage, self.scanning, self.pulse)
    }
    // Returns true only when the owning Tools entry should regain focus.
    pub fn input(&mut self, action: Action) -> Response {
        if action == Action::Back {
            if self.page.pending() {
                if self.page == Page::Connecting {
                    self.page = Page::Cancelling;
                    self.elapsed = 0;
                }
                return Response::Stay;
            }
            match self.page {
                Page::List | Page::Off | Page::Empty | Page::Busy => {
                    self.editor.clear();
                    return Response::Close;
                }
                Page::Info if self.info_entry => return Response::Close,
                Page::Forget => {
                    self.page = Page::Details;
                    self.selected = 1;
                }
                Page::Security => {
                    self.page = Page::Hidden;
                    self.editor.clear();
                }
                _ => self.list(),
            }
            return Response::Stay;
        }
        if self.page.pending() {
            return Response::Stay;
        }
        if self.page.keyboard() {
            return self
                .editor
                .input(action)
                .map_or(Response::Stay, Response::Edit);
        }
        if action == Action::Refresh && matches!(self.page, Page::List | Page::Empty) {
            self.scanning = true;
            self.elapsed = 0;
            return Response::Stay;
        }
        if action == Action::Auxiliary && self.page == Page::List {
            self.list_selection = self.slot(self.selected);
            self.info_entry = false;
            self.page = Page::Info;
            self.selected = 0;
            return Response::Stay;
        }
        let count = match self.page {
            Page::List => self.count + 1,
            Page::Security | Page::Details | Page::Forget | Page::Failed | Page::Empty => 2,
            _ => 1,
        };
        match action {
            Action::Up | Action::Left => {
                self.selected = self.selected.saturating_sub(1);
                return Response::Stay;
            }
            Action::Down | Action::Right => {
                self.selected = (self.selected + 1).min(count - 1);
                return Response::Stay;
            }
            Action::Confirm => {}
            _ => return Response::Stay,
        }
        match self.page {
            Page::List => {
                self.list_selection = self.slot(self.selected);
                if let Some(row) = self.row(self.selected) {
                    self.target = self.slot(self.selected);
                    self.target_security = row.security;
                    self.ssid.clear();
                    self.ssid.push_str(&self.name(self.selected));
                    if row.connected {
                        self.page = Page::Details;
                        self.selected = 0;
                    } else if row.saved || row.security == Security::Open {
                        self.connect();
                    } else if row.security == Security::Psk {
                        self.page = Page::Password;
                        self.editor.clear();
                    } else {
                        self.notice = "此网络需要预先配置认证信息";
                    }
                } else {
                    self.page = Page::Hidden;
                    self.editor.clear();
                }
            }
            Page::Off => {
                self.radio = true;
                self.list();
                self.scanning = true;
            }
            Page::Empty if self.selected == 1 => {
                self.page = Page::Hidden;
                self.editor.clear();
            }
            Page::Empty => {
                self.scanning = true;
                self.elapsed = 0;
            }
            Page::Security if self.selected == 1 => {
                self.target_security = Security::Open;
                self.connect();
            }
            Page::Security => {
                self.page = Page::Password;
                self.editor.clear();
            }
            Page::Details if self.selected == 1 => {
                self.page = Page::Forget;
                self.selected = 0;
            }
            Page::Details => {
                self.connected = None;
                self.list();
            }
            Page::Forget if self.selected == 1 => {
                if let Some(index) = self.target {
                    self.saved &= !(1 << index);
                    if self.connected == Some(index) {
                        self.connected = None;
                    }
                }
                self.list();
            }
            Page::Forget => {
                self.page = Page::Details;
                self.selected = 1;
            }
            Page::Failed if self.selected == 0 => {
                if self.scenario == Scenario::RestoreFailure {
                    self.page = Page::Restoring;
                    self.elapsed = 0;
                    self.notice = "";
                } else {
                    self.connect();
                }
            }
            Page::Busy => return Response::Close,
            Page::Success | Page::Info | Page::Failed => return self.input(Action::Back),
            _ => {}
        }
        Response::Stay
    }
    pub fn submit(&mut self, value: &str) -> bool {
        if !self.page.keyboard() || !self.editor.submit(self.page, value) {
            return false;
        }
        if self.page == Page::Hidden {
            self.ssid.clear();
            self.ssid.push_str(value);
            self.target_security = Security::Psk;
            self.target = Some(NETWORKS.len());
            self.page = Page::Security;
            self.selected = 0;
            self.editor.clear();
        } else {
            self.connect();
        }
        true
    }
    pub fn select(&mut self, index: usize) {
        if self.page.keyboard() {
            self.editor.selected = index.min(34);
        } else if !self.page.pending() {
            self.selected = index.min(if self.page == Page::List {
                self.count
            } else {
                1
            });
        }
    }
    pub fn selection(&self) -> usize {
        if self.page.keyboard() {
            self.editor.selected
        } else {
            self.selected
        }
    }
}

#[derive(Default)]
pub(super) struct Editor {
    pub selected: usize,
    mode: u8,
    visible: bool,
    pub error: &'static str,
}
impl Editor {
    pub fn clear(&mut self) {
        self.selected = 0;
        self.mode = 0;
        self.visible = false;
        self.error = "";
    }
    pub fn echo_label(&self) -> String {
        if self.visible { "隐藏" } else { "显示" }.into()
    }
    fn character(&self, index: usize) -> char {
        let text = match self.mode {
            2 => "1234567890!@#$%^&*()_+-=.:,/?~",
            3 => "[]{}\\\";<>|`!@#$%^&*()_+-=.:,/?~",
            _ => "qwertyuiopasdfghjkl'zxcvbnm.-_",
        };
        let ch = text.chars().nth(index).unwrap_or(' ');
        if self.mode == 1 {
            ch.to_ascii_uppercase()
        } else {
            ch
        }
    }
    pub fn label(&self, index: usize, hidden: bool) -> String {
        if index < 30 {
            return self.character(index).to_string();
        }
        match index {
            30 => {
                if self.mode == 1 {
                    "小写"
                } else {
                    "大写"
                }
            }
            31 => match self.mode {
                2 => "#+=",
                3 => "ABC",
                _ => "123",
            },
            32 => "空格",
            33 => "删除",
            _ => {
                if hidden {
                    "下一步"
                } else {
                    "连接"
                }
            }
        }
        .into()
    }
    pub fn text(&self, hidden: bool, value: &str) -> String {
        if value.is_empty() {
            return if hidden {
                "输入网络名称"
            } else {
                "输入 Wi-Fi 密码"
            }
            .into();
        }
        if hidden || self.visible {
            value.into()
        } else {
            "*".repeat(value.chars().count())
        }
    }
    fn submit(&mut self, page: Page, value: &str) -> bool {
        if page == Page::Hidden {
            if !(1..=32).contains(&value.len()) || value.contains(['\0', '\n', '\r']) {
                self.error = "网络名称需为 1–32 字节";
                return false;
            }
        } else if value.contains(['\0', '\n', '\r'])
            || !((8..=63).contains(&value.len())
                || value.len() == 64 && value.bytes().all(|ch| ch.is_ascii_hexdigit()))
        {
            self.error = "密码需 8–63 字节或 64 位十六进制";
            return false;
        }
        true
    }
    pub fn input(&mut self, action: Action) -> Option<Edit> {
        match action {
            Action::Up => {
                self.selected = if self.selected >= 30 {
                    20 + (self.selected - 30) * 2
                } else {
                    self.selected.saturating_sub(10)
                }
            }
            Action::Down => {
                self.selected = if (20..30).contains(&self.selected) {
                    30 + (self.selected - 20) / 2
                } else if self.selected >= 30 {
                    self.selected
                } else {
                    (self.selected + 10).min(29)
                }
            }
            Action::Left => self.selected = self.selected.saturating_sub(1),
            Action::Right => self.selected = (self.selected + 1).min(34),
            Action::Refresh => {
                self.error = "";
                return Some(Edit::Backspace);
            }
            Action::Auxiliary => self.visible = !self.visible,
            Action::Start => return Some(Edit::Submit),
            Action::Confirm => match self.selected {
                0..30 => {
                    self.error = "";
                    return Some(Edit::Insert(self.character(self.selected) as u8));
                }
                30 => self.mode = if self.mode == 1 { 0 } else { 1 },
                31 => {
                    self.mode = match self.mode {
                        2 => 3,
                        3 => 0,
                        _ => 2,
                    }
                }
                32 => {
                    self.error = "";
                    return Some(Edit::Insert(b' '));
                }
                33 => {
                    self.error = "";
                    return Some(Edit::Backspace);
                }
                _ => return Some(Edit::Submit),
            },
            Action::Back => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_span_stays_bounded_and_continuous_across_reversals() {
        let mut previous = loading_span(0);
        for now in (16..=6000).step_by(16) {
            let span = loading_span(now);
            assert!(span.0 >= Fixed::ZERO && span.1 <= Fixed::ONE);
            assert!(span.1 - span.0 >= Fixed::from_ratio(19, 100));
            assert!((span.0 - previous.0).abs() < Fixed::from_ratio(1, 20));
            assert!((span.1 - previous.1).abs() < Fixed::from_ratio(1, 20));
            previous = span;
        }
    }

    #[test]
    fn four_layers_cover_printable_ascii_and_apostrophe_follows_l() {
        let mut editor = Editor::default();
        let mut seen = [false; 95];
        seen[0] = true;
        for mode in 0..4 {
            editor.mode = mode;
            for index in 0..30 {
                let ch = editor.character(index);
                seen[ch as usize - 32] = true;
            }
        }
        assert!(seen.into_iter().all(|value| value));
        editor.mode = 0;
        assert_eq!(editor.character(18), 'l');
        assert_eq!(editor.character(19), '\'');
        editor.mode = 1;
        assert_eq!(editor.character(19), '\'');
    }

    #[test]
    fn editor_borrows_exact_text_and_returns_framework_edit_intents() {
        let mut editor = Editor::default();
        assert_eq!(editor.text(false, "  'camp'  "), "**********");
        editor.input(Action::Auxiliary);
        assert_eq!(editor.text(false, "  'camp'  "), "  'camp'  ");
        assert!(editor.submit(Page::Password, "  'camp'  "));
        assert!(!editor.submit(Page::Password, "short"));
        assert!(matches!(
            editor.input(Action::Refresh),
            Some(Edit::Backspace)
        ));
        assert!(matches!(editor.input(Action::Start), Some(Edit::Submit)));
        assert!(editor.submit(Page::Password, &"a".repeat(64)));
        assert!(!editor.submit(Page::Hidden, &"a".repeat(64)));
    }

    #[test]
    fn every_keyboard_selection_stays_in_range() {
        for start in 0..35 {
            for action in [Action::Up, Action::Down, Action::Left, Action::Right] {
                let mut editor = Editor {
                    selected: start,
                    ..Editor::default()
                };
                editor.input(action);
                assert!(editor.selected < 35);
            }
        }
    }

    #[test]
    fn cancellation_is_coalesced_restores_prior_connection_and_clears_draft() {
        let mut state = State::default();
        state.open(false);
        state.select(2);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Password);
        assert!(state.submit("password"));
        assert_eq!(state.page, Page::Connecting);
        state.input(Action::Back);
        state.tick(300);
        state.input(Action::Back);
        state.tick(400);
        assert_eq!(state.page, Page::Restoring);
        state.input(Action::Back);
        state.tick(700);
        assert_eq!(state.page, Page::List);
        assert_eq!(state.selected, 2);
        assert_eq!(state.connected, Some(0));
    }

    #[test]
    fn success_saved_reconnect_wrong_password_and_forget_have_distinct_paths() {
        let mut state = State::default();
        state.set_scenario(Scenario::WrongPassword);
        state.select(2);
        state.input(Action::Confirm);
        assert!(state.submit("password"));
        state.tick(700);
        state.tick(700);
        assert_eq!(state.page, Page::Password);
        assert_eq!(state.connected, Some(0));
        state.scenario = Scenario::Normal;
        assert!(state.submit("password"));
        state.tick(700);
        state.tick(700);
        assert_eq!(state.page, Page::Success);
        state.input(Action::Back);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Details);
        state.select(1);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Forget);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Details);
        assert_eq!(state.selected, 1);
        state.input(Action::Confirm);
        state.select(1);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::List);
        assert!(!state.row(2).unwrap().saved);
        assert!(state.connected.is_none());
    }

    #[test]
    fn hidden_network_flows_through_security_and_preserves_name_after_success() {
        let mut state = State::default();
        state.open(false);
        state.select(4);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Hidden);
        assert!(state.submit("  Hidden's Wi-Fi  "));
        assert_eq!(state.page, Page::Security);
        state.select(1);
        state.input(Action::Confirm);
        state.tick(700);
        state.tick(700);
        assert_eq!(state.fact(0), "  Hidden's Wi-Fi  ");
        state.input(Action::Back);
        assert_eq!(state.selected, 0);
        assert_eq!(state.name(0), "  Hidden's Wi-Fi  ");
        assert!(state.row(0).unwrap().connected);
    }

    #[test]
    fn off_empty_busy_info_and_refresh_keep_their_owners() {
        let mut state = State::default();
        state.set_scenario(Scenario::RadioOff);
        assert_eq!(state.page, Page::Off);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::List);
        state.set_scenario(Scenario::Empty);
        state.select(1);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Hidden);
        state.input(Action::Back);
        assert_eq!(state.page, Page::Empty);
        state.set_scenario(Scenario::Busy);
        assert!(matches!(state.input(Action::Back), Response::Close));
        state.set_scenario(Scenario::Normal);
        state.select(2);
        state.input(Action::Auxiliary);
        assert_eq!(state.page, Page::Info);
        assert!(matches!(state.input(Action::Back), Response::Stay));
        assert_eq!(state.selected, 2);
        state.open(true);
        assert!(matches!(state.input(Action::Back), Response::Close));
        state.open(false);
        state.input(Action::Refresh);
        state.tick(700);
        assert!(!state.scanning);
        state.select(3);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::List);
        assert!(!state.notice.is_empty());
    }

    #[test]
    fn list_window_and_fixtures_are_bounded_and_stable() {
        let mut state = State::default();
        for rows in 1..=8 {
            state.visible_rows = rows;
            for selected in 0..=state.count {
                state.selected = selected;
                let window = state.window();
                assert!(window.contains(&selected));
                assert!(window.len() <= rows);
                assert!(window.end <= state.count + 1);
            }
        }
        state.fixture(Page::Connecting);
        assert!(!state.tick(2000));
        assert_eq!(state.stage, 1);
    }

    #[test]
    fn mock_clock_only_publishes_visible_changes() {
        let mut state = State::default();
        state.open(false);
        assert!(!state.tick(16));
        assert!(state.tick(125));
        state.tick(700);
        assert!(!state.tick(16));
        state.select(1);
        state.input(Action::Confirm);
        assert!(!state.tick(16));
        assert!(state.tick(700));
        assert!(!state.tick(16));
    }

    #[test]
    fn connected_first_order_preserves_selected_network_identity() {
        let mut state = State::default();
        state.open(false);
        state.select(1);
        let name = state.name(1);
        state.input(Action::Confirm);
        state.tick(700);
        state.tick(700);
        state.input(Action::Back);
        assert_eq!(state.selected, 0);
        assert_eq!(state.name(state.selected), name);
        assert!(state.row(0).unwrap().connected);
        state.input(Action::Confirm);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::List);
        assert_eq!(state.selected, 1);
        assert_eq!(state.name(state.selected), name);
        state.input(Action::Confirm);
        assert_eq!(state.page, Page::Connecting);
        state.close();
        assert!(!state.scanning);
        assert_eq!(state.target, None);
    }
}
