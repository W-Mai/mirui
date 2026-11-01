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
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Self::Favorites => "收藏",
            Self::Platforms => "游戏平台",
            Self::Games => "游戏列表",
            Self::Search => "搜索游戏",
        }
    }
}

#[model]
pub struct Game {
    pub directory: bool,
    pub played: u64,
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
    pub fn directory(&self) -> bool {
        self.directory
    }
    pub fn played(&self) -> u64 {
        self.played
    }
    pub fn record_play(&mut self, order: u64) {
        self.played = order;
    }
    pub fn matches(&self, query: &str) -> bool {
        !self.directory
            && (self.name.contains(query)
                || query.is_empty()
                || self
                    .search
                    .as_bytes()
                    .windows(query.len())
                    .any(|word| word.eq_ignore_ascii_case(query.as_bytes())))
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
pub enum MenuAction {
    Play,
    Multiplayer,
    ToggleFavorite,
    CycleTheme,
    ToggleSound,
    ShowControls,
    ShowTools,
    OpenWifi,
    NetworkInfo,
    SystemInfo,
    FileManager,
    VolumeDown,
    VolumeUp,
    BrightnessDown,
    BrightnessUp,
    ManageNetwork,
    ShowExit,
    ShowPower,
    Exit,
    Poweroff,
    RestorePlay,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Game,
    System,
    Controls,
    Tools,
    Exit,
    Power,
    Network,
}

impl Overlay {
    fn items(self) -> &'static [(MenuAction, &'static str)] {
        use MenuAction::*;
        match self {
            Self::Game => &[
                (Play, "开始游戏"),
                (Multiplayer, "附近联机"),
                (ToggleFavorite, "收藏游戏"),
                (Back, "返回"),
            ],
            Self::System => &[
                (CycleTheme, "切换主题"),
                (ToggleSound, "音效开关"),
                (ShowControls, "系统控制"),
                (ManageNetwork, "管理直连网络"),
                (ShowExit, "返回原系统"),
                (ShowPower, "关机"),
            ],
            Self::Controls => &[
                (VolumeDown, "音量 -"),
                (VolumeUp, "音量 +"),
                (BrightnessDown, "亮度 -"),
                (BrightnessUp, "亮度 +"),
                (ShowTools, "Wi-Fi 与工具"),
                (Back, "返回"),
            ],
            Self::Tools => &[
                (OpenWifi, "Wi-Fi 设置"),
                (NetworkInfo, "网络信息"),
                (FileManager, "文件管理"),
                (SystemInfo, "系统信息"),
                (Back, "返回"),
            ],
            Self::Exit => &[(Back, "返回"), (Exit, "退出并恢复 Wi-Fi")],
            Self::Power => &[(Back, "返回"), (Poweroff, "确认关机")],
            Self::Network => &[
                (Play, "保留直连，开始游戏"),
                (RestorePlay, "恢复 Wi-Fi 后开始"),
                (Back, "返回"),
            ],
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub action: MenuAction,
    pub name: &'static str,
    pub enabled: bool,
    pub reason: &'static str,
    pub value: String,
}

pub const HOLD_MS: u16 = 700;

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
    pub keyboard: bool,
    #[observe]
    pub recent_sort: bool,
    play_count: u64,
    #[observe]
    pub overlay: Option<Overlay>,
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
    #[observe]
    catalog_selection: usize,
    #[observe]
    shelf_age: u16,
    #[observe]
    catalog_age: u16,
    #[observe]
    menu_age: u16,
    direction: PageDirection,
    origin: Screen,
    #[observe]
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
            directory: false,
            played: 0,
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
        })
        .chain(core::iter::once(Game {
            directory: true,
            played: 0,
            key: 8,
            name: "游戏库",
            search: "",
            platform: usize::MAX,
            multiplayer: false,
            favorite: true,
        }));
        let mut list = ModelList::default();
        list.load(games).expect("demo games");
        let mut result = Self {
            active: true,
            screen: Screen::Favorites,
            selected: 0,
            platform: 0,
            query: String::new(),
            query_revision: 0,
            keyboard: false,
            recent_sort: false,
            play_count: 0,
            overlay: None,
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
            shelf_age: 800,
            catalog_age: 800,
            menu_age: 800,
            direction: PageDirection::Forward,
            origin: Screen::Favorites,
            selected_game: None,
        };
        result.sort_games();
        result
    }
}

#[model]
impl Launcher {
    fn sort_games(&mut self) {
        let mut order: alloc::vec::Vec<_> = self
            .games
            .entries()
            .map(|(id, game)| (id, game.directory(), game.name(), game.played()))
            .collect();
        order.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| {
                    if self.recent_sort {
                        b.3.cmp(&a.3)
                    } else {
                        core::cmp::Ordering::Equal
                    }
                })
                .then_with(|| a.2.cmp(&b.2))
        });
        for (index, (id, _, _, _)) in order.into_iter().enumerate() {
            self.games.move_to(id, index).expect("existing game member");
        }
    }
    pub fn sort_favorites(&mut self) {
        self.recent_sort = !self.recent_sort;
        self.sort_games();
    }
    pub fn toggle_selected_favorite(&mut self) {
        for game in self.games.iter_mut() {
            if game.key() as usize == self.selected && !game.directory() {
                game.toggle_favorite();
            }
        }
    }
    pub(super) fn animate(&mut self, delta: u16) {
        if self.overlay.is_some() {
            let value = self.menu_age.saturating_add(delta).min(800);
            if self.menu_age != value {
                self.menu_age = value;
            }
        } else if self.screen == Screen::Favorites {
            let value = self.shelf_age.saturating_add(delta).min(800);
            if self.shelf_age != value {
                self.shelf_age = value;
            }
        } else {
            let value = self.catalog_age.saturating_add(delta).min(800);
            if self.catalog_age != value {
                self.catalog_age = value;
            }
        }
    }
    #[observe]
    pub(super) fn selection_age(&self) -> u16 {
        if self.overlay.is_some() {
            self.menu_age
        } else if self.screen == Screen::Favorites {
            self.shelf_age
        } else {
            self.catalog_age
        }
    }
    #[observe]
    pub(super) fn card_motion(&self) -> super::motion::Frame {
        super::motion::Frame {
            page: super::data::Page::Games,
            selected: Some(self.catalog_selection),
            selection: self.catalog_age,
            pulse: self.catalog_age,
            ..Default::default()
        }
    }
    #[observe]
    pub(super) fn shelf_card_motion(&self) -> super::motion::Frame {
        super::motion::Frame {
            page: super::data::Page::Games,
            selected: Some(self.shelf_selection),
            selection: self.shelf_age,
            pulse: self.shelf_age,
            ..Default::default()
        }
    }
    pub fn query(&self) -> String {
        self.query.clone()
    }
    pub fn search_empty(&self) -> bool {
        self.query.trim().is_empty() || self.games.iter().all(|game| !game.matches(&self.query))
    }
    #[observe]
    pub fn transition(&self) -> PageTransition {
        PageTransition::slide(crate::anim::SNAPPY).with_direction(self.direction)
    }

    pub fn open(&mut self, screen: Screen) {
        self.cancel_hold();
        self.overlay = None;
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
        self.keyboard = screen == Screen::Search;
        self.selected = if screen == Screen::Favorites {
            self.shelf_selection
        } else if screen == self.catalog_screen {
            self.catalog_selection
        } else {
            0
        };
        if screen != Screen::Favorites {
            if self.catalog_screen != screen {
                self.catalog_age = 0;
            }
            self.catalog_screen = screen;
            self.catalog_selection = self.selected;
        }
        if self.keyboard {
            self.selected = 100;
        }
    }

    pub fn select(&mut self, index: usize) {
        if self.selected != index {
            if self.screen == Screen::Favorites {
                self.shelf_age = 0;
            } else {
                self.catalog_age = 0;
            }
        }
        self.cancel_hold();
        self.selected = index;
        if self.screen == Screen::Favorites {
            self.shelf_selection = index;
        } else {
            self.catalog_selection = index;
        }
    }

    pub fn choose_game(&mut self, key: u32, index: usize) {
        if self
            .games
            .iter()
            .any(|game| game.key() == key && game.directory())
        {
            self.open(Screen::Platforms);
            return;
        }
        self.cancel_hold();
        self.selected_game = Some(key);
        self.select(index);
        if self.screen == Screen::Favorites {
            self.shelf_selection = index;
        }
        self.show_overlay(Overlay::Game);
    }

    pub fn dismiss_menu(&mut self) {
        self.overlay = None;
    }
    pub fn select_action(&mut self, index: usize) {
        let index = index.min(self.item_count().saturating_sub(1));
        if self.menu_selection != index {
            self.menu_age = 0;
        }
        self.menu_selection = index;
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.query_revision = self.query_revision.wrapping_add(1);
        self.keyboard = false;
        self.select_result();
    }

    pub fn type_key(&mut self, key: &'static str) {
        match key {
            "退格" => {
                self.query.pop();
            }
            "清空" => self.query.clear(),
            "空格" => self.query.push(' '),
            "结果" => {
                self.keyboard = false;
            }
            _ => self.query.push_str(key),
        }
        self.query_revision = self.query_revision.wrapping_add(1);
        if !self.keyboard {
            self.select_result();
        }
    }

    fn select_result(&mut self) {
        let query = self.query.to_lowercase();
        let key = self
            .games
            .iter()
            .find(|game| !query.trim().is_empty() && game.matches(&query))
            .map_or(0, |game| game.key() as usize);
        self.select(key);
    }

    pub fn edit_query(&mut self) {
        self.keyboard = true;
        self.selected = 100;
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

    pub fn toggle_favorite(&mut self) {
        if let Some(key) = self.selected_game {
            for game in self.games.iter_mut() {
                if game.key() == key {
                    game.toggle_favorite();
                }
            }
        }
    }

    pub fn launch(&mut self) {
        self.play_count = self.play_count.saturating_add(1);
        for game in self.games.iter_mut() {
            if Some(game.key()) == self.selected_game {
                game.record_play(self.play_count);
            }
        }
        if self.recent_sort {
            self.sort_games();
        }
        self.overlay = None;
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

    pub fn adjust(&mut self, tool: MenuAction) {
        match tool {
            MenuAction::ToggleSound => self.toggle_sound(),
            MenuAction::CycleTheme => self.palette = (self.palette + 1) % 3,
            MenuAction::VolumeDown => self.volume = self.volume.saturating_sub(10),
            MenuAction::VolumeUp => self.volume = self.volume.saturating_add(10).min(100),
            MenuAction::BrightnessDown => self.brightness = self.brightness.saturating_sub(10),
            MenuAction::BrightnessUp => {
                self.brightness = self.brightness.saturating_add(10).min(100)
            }
            _ => return,
        }
        self.notice = "LOCAL DEMO: settings updated";
    }

    pub fn result(&mut self, notice: &'static str) {
        self.notice = notice;
    }

    pub fn begin_hold(&mut self, key: u32) {
        if self.held_game.is_none()
            && self.active
            && self.overlay.is_none()
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

    #[observe]
    pub fn menu(&self) -> bool {
        self.overlay.is_some()
    }
    #[observe]
    pub fn item_count(&self) -> usize {
        self.overlay.map_or(0, |overlay| overlay.items().len())
    }
    fn read_title(&self) -> String {
        match self.overlay {
            Some(Overlay::Game) => self
                .games
                .iter()
                .find(|game| Some(game.key()) == self.selected_game)
                .map(|game| game.name())
                .unwrap_or_else(|| "游戏操作".into()),
            Some(Overlay::System) => "启动器设置".into(),
            Some(Overlay::Controls) => "系统控制".into(),
            Some(Overlay::Tools) => "系统工具".into(),
            Some(Overlay::Exit) => "返回原系统？".into(),
            Some(Overlay::Power) => "关闭掌机？".into(),
            Some(Overlay::Network) => "当前保留着直连网络".into(),
            None => String::new(),
        }
    }
    fn read_item(&self, index: usize) -> Option<MenuItem> {
        use MenuAction::*;
        let (action, mut name) = *self.overlay?.items().get(index)?;
        let mut enabled = true;
        let mut reason = "";
        let mut value = String::new();
        match action {
            Multiplayer => {
                enabled = self
                    .games
                    .iter()
                    .any(|game| Some(game.key()) == self.selected_game && game.multiplayer());
                if !enabled {
                    reason = "此游戏暂未接入联机";
                }
            }
            ToggleFavorite => {
                if self
                    .games
                    .iter()
                    .any(|game| Some(game.key()) == self.selected_game && game.favorite())
                {
                    name = "移出收藏";
                }
            }
            ManageNetwork => {
                enabled = self.connected;
                if !enabled {
                    reason = "尚未保留直连网络";
                }
            }
            VolumeDown | VolumeUp => value = alloc::format!("{}%", self.volume),
            BrightnessDown | BrightnessUp => value = alloc::format!("{}%", self.brightness),
            ToggleSound => {
                value = if self.sound {
                    "开".into()
                } else {
                    "关".into()
                }
            }
            CycleTheme => value = ["森林", "落日", "夜色"][self.palette].into(),
            _ => {}
        }
        Some(MenuItem {
            action,
            name,
            enabled,
            reason,
            value,
        })
    }
    pub fn show_overlay(&mut self, overlay: Overlay) {
        self.cancel_hold();
        self.overlay = Some(overlay);
        self.menu_selection = 0;
        self.menu_age = 0;
        self.notice = "";
    }
    pub fn back(&mut self) {
        self.cancel_hold();
        if let Some(overlay) = self.overlay {
            match overlay {
                Overlay::Tools => self.show_overlay(Overlay::Controls),
                Overlay::Controls | Overlay::Exit | Overlay::Power => {
                    self.show_overlay(Overlay::System)
                }
                _ => self.overlay = None,
            }
            return;
        }
        if self.screen == Screen::Search && self.keyboard {
            self.keyboard = false;
            self.selected = 0;
            return;
        }
        let target = if self.screen == Screen::Games {
            self.origin
        } else {
            Screen::Favorites
        };
        self.open(target);
        self.direction = PageDirection::Backward;
    }

    pub fn close(&mut self) {
        self.cancel_hold();
        self.active = false;
        self.overlay = None;
    }
    pub fn resume(&mut self) {
        self.active = true;
    }
}

impl LauncherHandle {
    pub(super) fn shelf_counter(&self) -> Option<(usize, usize)> {
        let games = self.games();
        let mut count = 0;
        let mut position = 0;
        for game in games.iter().filter(|game| game.favorite()) {
            count += 1;
            if game.key() as usize == self.shelf_selection() {
                position = count;
            }
        }
        (count > 0).then_some((position, count))
    }
    pub(super) fn browser_confirm(&self) -> &'static str {
        if self.keyboard() {
            return "输入";
        }
        let available = match self.screen() {
            Screen::Platforms => true,
            Screen::Favorites => self.games().iter().any(|game| game.favorite()),
            Screen::Games => self
                .games()
                .iter()
                .any(|game| game.platform() == self.platform()),
            Screen::Search => {
                let query = self.query().to_lowercase();
                !query.trim().is_empty() && self.games().iter().any(|game| game.matches(&query))
            }
        };
        if available { "选择" } else { "" }
    }
    pub(super) fn browser_back(&self) -> &'static str {
        if self.screen() == Screen::Games {
            "上级"
        } else {
            "返回"
        }
    }
    pub(super) fn browser_hint(&self) -> &'static str {
        if !self.notice().is_empty() {
            return self.notice();
        }
        match self.screen() {
            Screen::Favorites
                if self
                    .games()
                    .iter()
                    .any(|game| game.key() as usize == self.selected() && game.directory()) =>
            {
                "Y 排序"
            }
            Screen::Favorites => "X 游玩 / 长按联机 · Y 排序",
            Screen::Platforms => "Y 搜索",
            Screen::Games => "X 游玩 / 长按联机 · Y 收藏",
            Screen::Search if self.keyboard() => "Y 退格 · START 结果",
            Screen::Search => {
                let query = self.query().to_lowercase();
                if !query.trim().is_empty()
                    && self
                        .games()
                        .iter()
                        .any(|game| game.key() as usize == self.selected() && game.matches(&query))
                {
                    "X 游玩 · Y 收藏 · START 键盘"
                } else {
                    "Y 键盘"
                }
            }
        }
    }
    pub fn overlay_title(&self) -> String {
        self.overlay();
        self.selected_game();
        self.read_title()
    }
    pub fn item(&self, index: usize) -> Option<MenuItem> {
        self.overlay();
        self.sound();
        self.connected();
        self.volume();
        self.brightness();
        self.palette();
        let key = self.selected_game();
        if let Some(game) = self.games().iter().find(|game| Some(game.key()) == key) {
            game.favorite();
        }
        self.read_item(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_layers_keep_independent_motion_and_collection_identity() {
        let mut launcher = Launcher::default();
        launcher.select(4);
        launcher.animate(70);
        let shelf = launcher.shelf_card_motion();
        launcher.open(Screen::Platforms);
        launcher.select(2);
        launcher.animate(90);
        assert_eq!(launcher.shelf_card_motion(), shelf);
        let catalog = launcher.card_motion();
        launcher.show_overlay(Overlay::System);
        launcher.select_action(2);
        launcher.animate(300);
        assert_eq!(launcher.shelf_card_motion(), shelf);
        assert_eq!(launcher.card_motion(), catalog);
        launcher.dismiss_menu();
        launcher.open(Screen::Favorites);
        let identities: alloc::vec::Vec<_> = launcher
            .games
            .entries()
            .map(|(id, game)| (game.key(), id))
            .collect();
        launcher.sort_favorites();
        for (key, id) in identities {
            assert_eq!(launcher.games.get_by_id(id).unwrap().key(), key);
        }
        launcher.choose_game(8, 8);
        assert_eq!(launcher.screen, Screen::Platforms);
        assert!(!launcher.menu());
    }
    #[test]
    fn nested_overlays_preserve_the_browser_and_return_to_their_parent() {
        let mut launcher = Launcher::default();
        launcher.choose_platform(2);
        launcher.select(3);
        launcher.show_overlay(Overlay::System);
        launcher.show_overlay(Overlay::Controls);
        launcher.show_overlay(Overlay::Tools);
        launcher.back();
        assert_eq!(launcher.overlay, Some(Overlay::Controls));
        launcher.back();
        assert_eq!(launcher.overlay, Some(Overlay::System));
        launcher.back();
        assert_eq!(launcher.overlay, None);
        assert_eq!((launcher.screen, launcher.selected), (Screen::Games, 3));
        launcher.show_overlay(Overlay::Power);
        launcher.back();
        assert_eq!(launcher.overlay, Some(Overlay::System));
        assert_eq!(launcher.item_count(), 6);
        assert!(!launcher.read_item(3).unwrap().enabled);
        assert!(!launcher.read_item(3).unwrap().reason.is_empty());
    }
    #[test]
    fn game_overlay_uses_four_actions_and_preserves_identity() {
        let mut launcher = Launcher::default();
        launcher.choose_game(2, 2);
        assert_eq!(launcher.item_count(), 4);
        assert!(!launcher.read_item(1).unwrap().enabled);
        launcher.back();
        assert!(!launcher.menu());
        assert_eq!(launcher.selected_game, Some(2));
        launcher.close();
        launcher.resume();
        assert!(launcher.active);
    }
}
