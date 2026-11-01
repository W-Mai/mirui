use crate::core::model::ModelList;
use crate::prelude::*;
use crate::ui::widgets::navigation::{PageDirection, PageTransition};
use alloc::string::String;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Favorites,
    Platforms,
    Games,
    Search,
    Tools,
    Wifi,
    Files,
    Manual,
    Info,
    Result,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Self::Favorites => "收藏",
            Self::Platforms => "游戏平台",
            Self::Games => "游戏列表",
            Self::Search => "搜索游戏",
            Self::Tools => "系统工具",
            Self::Wifi => "Wi-Fi",
            Self::Files => "文件管理",
            Self::Manual => "游戏说明",
            Self::Info => "游戏信息",
            Self::Result => "LOCAL DEMO",
        }
    }
}

#[model]
pub struct Game {
    pub key: u32,
    pub name: &'static str,
    pub search: &'static str,
    pub platform: usize,
    pub multiplayer: bool,
    #[observe]
    pub favorite: bool,
}

#[model]
impl Game {
    pub fn key(&self) -> u32 {
        self.key
    }
    pub fn name(&self) -> String {
        self.name.into()
    }
    pub fn platform(&self) -> usize {
        self.platform
    }
    pub fn matches(&self, query: &str) -> bool {
        self.name.contains(query) || self.search.contains(query)
    }
    pub fn multiplayer(&self) -> bool {
        self.multiplayer
    }
    pub fn toggle_favorite(&mut self) {
        self.favorite = !self.favorite;
    }
}

pub const PLATFORMS: [(&str, &str); 6] = [
    ("FC / NES", "nes"),
    ("SUPER FAMICOM", "snes"),
    ("GAME BOY", "gb"),
    ("MEGA DRIVE", "megadrive"),
    ("PLAYSTATION", "psx"),
    ("ARCADE", "arcade"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Wifi,
    Files,
    Sound,
    Theme,
    VolumeDown,
    VolumeUp,
    BrightnessDown,
    BrightnessUp,
    Network,
    NetworkInfo,
    Controls,
    SystemInfo,
    RestorePlay,
    Exit,
    Poweroff,
    Back,
}

pub const TOOLS: [(Tool, &str, &str); 16] = [
    (Tool::Wifi, "WIFI", "Wi-Fi"),
    (Tool::Files, "FILES", "文件管理"),
    (Tool::Sound, "SOUND", "音效"),
    (Tool::Theme, "THEME", "主题"),
    (Tool::VolumeDown, "VOL -", "音量 -"),
    (Tool::VolumeUp, "VOL +", "音量 +"),
    (Tool::BrightnessDown, "LIGHT -", "亮度 -"),
    (Tool::BrightnessUp, "LIGHT +", "亮度 +"),
    (Tool::Network, "LINK", "附近联机"),
    (Tool::NetworkInfo, "STATUS", "网络信息"),
    (Tool::Controls, "KEYS", "操作说明"),
    (Tool::SystemInfo, "SYSTEM", "系统信息"),
    (Tool::RestorePlay, "RESUME", "继续游玩"),
    (Tool::Exit, "EXIT", "退出"),
    (Tool::Poweroff, "POWER", "关机"),
    (Tool::Back, "BACK", "返回"),
];

pub const HOLD_MS: u16 = 600;

#[model]
pub struct Launcher {
    #[observe]
    pub active: bool,
    #[observe]
    pub screen: Screen,
    #[observe]
    pub selected: usize,
    #[observe]
    pub platform: usize,
    pub query: String,
    #[observe]
    pub query_revision: u32,
    #[observe]
    pub menu: bool,
    #[observe]
    pub menu_selection: usize,
    #[observe]
    pub notice: &'static str,
    #[observe]
    pub sound: bool,
    #[observe]
    pub connected: bool,
    #[observe]
    pub volume: u8,
    #[observe]
    pub brightness: u8,
    #[observe]
    pub palette: usize,
    #[observe]
    pub hold_progress: Fixed,
    held_game: Option<u32>,
    held_ms: u16,
    pub games: model!(owned ModelList<Game>),
    pub(super) shelf: model!(owned super::shelf::Shelf),
    pub(super) deck: model!(owned super::deck::Deck),
    #[observe]
    pub shelf_selection: usize,
    #[observe]
    pub catalog_screen: Screen,
    catalog_selection: usize,
    #[observe]
    pub(super) selection_age: u16,
    #[observe]
    pub(super) pulse_age: u16,
    direction: PageDirection,
    origin: Screen,
    previous: Screen,
    previous_selection: usize,
    selected_game: Option<u32>,
}

impl Default for Launcher {
    fn default() -> Self {
        let games = [
            ("双人冒险", 0, true),
            ("街头赛车", 1, true),
            ("方块世界", 2, false),
            ("旅途中的朋友", 3, true),
            ("周末一起玩", 4, true),
            ("经典游戏", 5, true),
            ("新的地图", 0, true),
            ("速度比赛", 1, true),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, platform, multiplayer))| Game {
            key: index as u32,
            search: [
                "adventure",
                "racing",
                "blocks",
                "journey",
                "weekend",
                "classic",
                "map",
                "speed",
            ][index],
            name,
            platform,
            multiplayer,
            favorite: index < 6,
        });
        let mut list = ModelList::default();
        list.load(games).expect("demo games");
        Self {
            active: true,
            screen: Screen::Favorites,
            selected: 0,
            platform: 0,
            query: String::new(),
            query_revision: 0,
            menu: false,
            menu_selection: 0,
            notice: "",
            sound: true,
            connected: false,
            volume: 60,
            brightness: 80,
            palette: 0,
            hold_progress: Fixed::ZERO,
            held_game: None,
            held_ms: 0,
            games: list.into(),
            shelf: super::shelf::Shelf::default().into(),
            deck: super::deck::Deck::default().into(),
            shelf_selection: 0,
            catalog_screen: Screen::Platforms,
            catalog_selection: 0,
            selection_age: 800,
            pulse_age: 800,
            direction: PageDirection::Forward,
            origin: Screen::Favorites,
            previous: Screen::Favorites,
            previous_selection: 0,
            selected_game: None,
        }
    }
}

#[model]
impl Launcher {
    pub(super) fn animate(&mut self, delta: u16) {
        let age = self.selection_age.saturating_add(delta).min(800);
        if age != self.selection_age {
            self.selection_age = age;
        }
        let pulse = self.pulse_age.saturating_add(delta).min(800);
        if pulse != self.pulse_age {
            self.pulse_age = pulse;
        }
    }
    #[observe]
    pub(super) fn card_motion(&self) -> super::motion::Frame {
        super::motion::Frame {
            page: super::data::Page::Games,
            selected: Some(self.selected),
            selection: self.selection_age,
            pulse: self.pulse_age,
            ..Default::default()
        }
    }
    pub fn query(&self) -> String {
        self.query.clone()
    }
    #[observe]
    pub fn transition(&self) -> PageTransition {
        PageTransition::slide(crate::anim::SNAPPY).with_direction(self.direction)
    }

    pub fn open(&mut self, screen: Screen) {
        self.cancel_hold();
        self.menu = false;
        self.notice = "";
        self.direction = if screen == Screen::Favorites {
            PageDirection::Backward
        } else {
            PageDirection::Forward
        };
        if self.screen == Screen::Favorites {
            self.shelf_selection = self.selected;
        } else {
            self.catalog_selection = self.selected;
        }
        self.screen = screen;
        self.selected = if screen == Screen::Favorites {
            self.shelf_selection
        } else if screen == self.catalog_screen {
            self.catalog_selection
        } else {
            0
        };
        if screen != Screen::Favorites {
            self.catalog_screen = screen;
        }
    }

    pub fn select(&mut self, index: usize) {
        if self.selected != index {
            self.selection_age = 0;
            self.pulse_age = 0;
        }
        self.cancel_hold();
        self.selected = index;
        if self.screen == Screen::Favorites {
            self.shelf_selection = index;
        }
    }

    pub fn choose_game(&mut self, key: u32, index: usize) {
        self.cancel_hold();
        self.selected_game = Some(key);
        self.selected = index;
        if self.screen == Screen::Favorites {
            self.shelf_selection = index;
        }
        self.menu = true;
        self.menu_selection = 0;
    }

    pub fn dismiss_menu(&mut self) {
        self.menu = false;
    }
    pub fn select_action(&mut self, index: usize) {
        self.menu_selection = index.min(5);
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.query_revision = self.query_revision.wrapping_add(1);
        self.selected = 0;
    }

    pub fn type_key(&mut self, key: &'static str) {
        match key {
            "DEL" => {
                self.query.pop();
            }
            "CLEAR" => self.query.clear(),
            "SPACE" => self.query.push(' '),
            _ => self.query.push_str(key),
        }
        self.query_revision = self.query_revision.wrapping_add(1);
        self.selected = 0;
    }

    pub fn game_key(&self) -> Option<u32> {
        self.selected_game
    }

    pub fn choose_platform(&mut self, index: usize) {
        if index >= PLATFORMS.len() {
            return;
        }
        self.platform = index;
        self.origin = Screen::Platforms;
        self.open(Screen::Games);
    }
    pub fn browse_files(&mut self, index: usize) {
        self.choose_platform(index);
        self.origin = Screen::Files;
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(key) = self.selected_game {
            for game in self.games.iter_mut() {
                if game.key() == key {
                    game.toggle_favorite();
                }
            }
        }
    }

    pub fn show_game_page(&mut self, screen: Screen) {
        self.previous = self.screen;
        self.previous_selection = self.selected;
        self.open(screen);
    }

    pub fn launch(&mut self) {
        self.show_game_page(Screen::Result);
        self.notice = "单机游玩 · LOCAL DEMO";
    }

    pub fn network(&mut self) {
        self.connected = !self.connected;
        self.notice = if self.connected {
            "已连接 · LOCAL DEMO"
        } else {
            "已断开 · LOCAL DEMO"
        };
    }

    pub fn toggle_sound(&mut self) {
        self.sound = !self.sound;
    }

    pub fn adjust_tool(&mut self, tool: Tool) {
        match tool {
            Tool::Sound => self.toggle_sound(),
            Tool::Theme => self.palette = (self.palette + 1) % 3,
            Tool::VolumeDown => self.volume = self.volume.saturating_sub(10),
            Tool::VolumeUp => self.volume = self.volume.saturating_add(10).min(100),
            Tool::BrightnessDown => self.brightness = self.brightness.saturating_sub(10),
            Tool::BrightnessUp => self.brightness = self.brightness.saturating_add(10).min(100),
            _ => return,
        }
        self.notice = "LOCAL DEMO: settings updated";
    }

    pub fn result(&mut self, notice: &'static str) {
        self.show_game_page(Screen::Result);
        self.notice = notice;
    }

    pub fn begin_hold(&mut self, key: u32) {
        if self.held_game.is_none()
            && self.active
            && !self.menu
            && matches!(
                self.screen,
                Screen::Favorites | Screen::Games | Screen::Search
            )
        {
            self.held_game = Some(key);
            self.held_ms = 0;
            self.hold_progress = Fixed::ZERO;
        }
    }

    pub fn advance_hold(&mut self, dt: u16) {
        if self.held_game.is_some() {
            self.held_ms = self.held_ms.saturating_add(dt).min(HOLD_MS);
            self.hold_progress = Fixed::from_ratio(self.held_ms as i32, HOLD_MS as i32);
        }
    }

    pub fn held_play(&self) -> Option<(u32, bool)> {
        self.held_game.map(|key| (key, self.held_ms >= HOLD_MS))
    }

    pub fn cancel_hold(&mut self) {
        self.held_game = None;
        self.held_ms = 0;
        self.hold_progress = Fixed::ZERO;
    }

    pub fn holding(&self) -> bool {
        self.held_game.is_some()
    }

    pub fn tool(&mut self, screen: Screen) {
        self.previous = Screen::Tools;
        self.open(screen);
    }

    pub fn back(&mut self) {
        self.cancel_hold();
        if self.menu {
            self.menu = false;
            return;
        }
        let target = match self.screen {
            Screen::Games => self.origin,
            Screen::Wifi | Screen::Files => Screen::Tools,
            Screen::Info | Screen::Manual | Screen::Result => self.previous,
            _ => Screen::Favorites,
        };
        self.open(target);
        self.direction = PageDirection::Backward;
        if target == self.previous {
            self.selected = self.previous_selection;
        }
    }

    pub fn close(&mut self) {
        self.cancel_hold();
        self.active = false;
        self.menu = false;
    }
    pub fn resume(&mut self) {
        self.active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_and_game_details_return_to_their_own_context() {
        let mut launcher = Launcher::default();
        launcher.choose_platform(2);
        assert_eq!(launcher.screen, Screen::Games);
        launcher.selected = 3;
        launcher.show_game_page(Screen::Info);
        launcher.back();
        assert_eq!((launcher.screen, launcher.selected), (Screen::Games, 3));
        launcher.back();
        assert_eq!(launcher.screen, Screen::Platforms);
        launcher.open(Screen::Tools);
        launcher.tool(Screen::Wifi);
        launcher.network();
        launcher.back();
        assert_eq!(launcher.screen, Screen::Tools);
        assert!(launcher.connected);
        launcher.back();
        assert_eq!(launcher.screen, Screen::Favorites);
    }

    #[test]
    fn closing_actions_does_not_navigate_or_lose_the_selected_game() {
        let mut launcher = Launcher::default();
        launcher.choose_game(4, 4);
        launcher.back();
        assert!(!launcher.menu);
        assert_eq!(launcher.screen, Screen::Favorites);
        assert_eq!(launcher.selected, 4);
        launcher.close();
        launcher.resume();
        assert!(launcher.active);
        assert_eq!(launcher.selected_game, Some(4));
    }
}
