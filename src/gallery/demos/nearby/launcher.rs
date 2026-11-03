use crate::anim::{Tween, TweenConfig, ease};
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
    pub platform: Option<usize>,
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
    pub fn platform(&self) -> Option<usize> {
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
    ShowGameDetails,
    ShowCoreDetails,
    ShowFirmwareDetails,
    ShowSaveDetails,
    ExpandDetail,
    CycleTheme,
    ToggleSound,
    ShowControls,
    ShowTools,
    OpenWifi,
    NetworkInfo,
    SystemInfo,
    FileManager,
    Volume,
    Brightness,
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
    Details(super::details::Section),
}

impl Overlay {
    fn items(self) -> &'static [(MenuAction, &'static str)] {
        use MenuAction::*;
        match self {
            Self::Game => &[
                (Play, "开始游戏"),
                (Multiplayer, "附近联机"),
                (ToggleFavorite, "收藏游戏"),
                (ShowGameDetails, "游戏详情"),
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
                (Volume, "音量"),
                (Brightness, "亮度"),
                (ShowTools, "系统工具"),
                (Back, "更多设置"),
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
            Self::Details(super::details::Section::Overview) => &[
                (ShowCoreDetails, "运行核心"),
                (ShowFirmwareDetails, "BIOS 文件"),
                (ShowSaveDetails, "存档与配置"),
            ],
            Self::Details(_) => &[],
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
    pub level: Option<Level>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemLevel {
    Volume,
    Brightness,
}

impl SystemLevel {
    pub const ALL: [Self; 2] = [Self::Volume, Self::Brightness];
    pub const fn index(self) -> usize {
        match self {
            Self::Volume => 0,
            Self::Brightness => 1,
        }
    }
    pub const fn minimum(self) -> u8 {
        match self {
            Self::Volume => 0,
            Self::Brightness => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LevelStatus {
    #[default]
    Ready,
    Pending,
    Reading,
    Unavailable(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub value: Option<u8>,
    pub minimum: u8,
    pub maximum: u8,
    pub step: u8,
    pub muted: bool,
    pub pending: bool,
}

const LEVEL_MOTION: TweenConfig = TweenConfig::new(150, &ease::OUT_CUBIC);

pub const HOLD_MS: u16 = 700;
const FEEDBACK_MS: u16 = 130;
const MAX_FRAME_GAP_MS: u16 = 250;

#[derive(Default)]
enum Gesture {
    #[default]
    Idle,
    Pressed {
        key: u32,
        elapsed: u16,
    },
    Consumed,
}

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
    pub level_status: [LevelStatus; 2],
    #[observe]
    pub level_positions: [Fixed; 2],
    level_motion: [Tween; 2],
    #[observe]
    pub palette: usize,
    #[observe]
    pub hold_progress: Fixed,
    gesture: Gesture,
    pending_play: Option<(u32, bool)>,
    pub games: model!(owned ModelList<Game>),
    pub(super) shelf: model!(owned super::shelf::Shelf),
    pub(super) list_layout: model!(owned super::list_layout::ListLayout),
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
    details: super::details::State,
    #[observe]
    detail_revision: u64,
    #[observe]
    pub detail_offset: Fixed,
    #[observe]
    detail_geometry: u64,
    #[cfg(feature = "audio")]
    audio_feedback: super::sound::Feedback,
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
            platform: Some(platform),
            multiplayer,
            favorite: index < 6,
        })
        .chain(core::iter::once(Game {
            directory: true,
            played: 0,
            key: 8,
            name: "游戏库",
            search: "",
            platform: None,
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
            level_status: [LevelStatus::Ready; 2],
            level_positions: [Fixed::from_int(60), Fixed::from_int(80)],
            level_motion: [
                Tween::preset(60.into(), 60.into(), LEVEL_MOTION),
                Tween::preset(80.into(), 80.into(), LEVEL_MOTION),
            ],
            palette: 0,
            hold_progress: Fixed::ZERO,
            gesture: Gesture::Idle,
            pending_play: None,
            games: list.into(),
            shelf: super::shelf::Shelf::default().into(),
            list_layout: super::list_layout::ListLayout::default().into(),
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
            details: super::details::State::default(),
            detail_revision: 0,
            detail_offset: Fixed::ZERO,
            detail_geometry: 0,
            #[cfg(feature = "audio")]
            audio_feedback: super::sound::Feedback::default(),
        };
        result.sort_games();
        result
    }
}

#[model]
impl Launcher {
    #[cfg(feature = "audio")]
    pub(super) fn bind_audio(&mut self, state: Option<crate::audio::AudioStateSignal>) {
        self.audio_feedback.bind(state);
    }

    #[cfg(feature = "audio")]
    pub(super) fn advance_audio(&mut self, delta: u16) {
        self.audio_feedback.advance(delta);
    }

    #[cfg(feature = "audio")]
    pub(super) fn emit_sound(&mut self, cue: super::sound::Cue) {
        self.audio_feedback.emit(cue);
    }

    #[cfg(feature = "audio")]
    #[effects]
    fn take_sound(&mut self) -> Option<super::sound::Effect> {
        self.audio_feedback.take()
    }

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
        #[cfg(feature = "audio")]
        self.audio_feedback.emit(super::sound::Cue::Confirm);
    }
    pub fn toggle_selected_favorite(&mut self) {
        let mut changed = false;
        for game in self.games.iter_mut() {
            if game.key() as usize == self.selected && !game.directory() {
                game.toggle_favorite();
                changed = true;
            }
        }
        if changed {
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
    }
    pub(super) fn animate(&mut self, delta: u16) {
        #[cfg(feature = "audio")]
        self.audio_feedback.advance(delta);
        for (index, control) in SystemLevel::ALL.into_iter().enumerate() {
            if let Some(value) = self
                .read_level(control.index())
                .and_then(|level| level.value)
            {
                self.level_motion[index].retarget(i32::from(value).into(), LEVEL_MOTION);
            }
            self.level_motion[index].tick(delta);
        }
        self.level_positions = self.level_motion.map(|motion| motion.value());
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
    fn query_value(&self) -> String {
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
        #[cfg(feature = "audio")]
        if self.screen != screen {
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
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

    pub fn toggle_tab(&mut self) {
        self.open(if self.screen == Screen::Favorites {
            self.catalog_screen
        } else {
            Screen::Favorites
        });
    }

    pub fn select(&mut self, index: usize) {
        #[cfg(feature = "audio")]
        if self.selected != index {
            self.audio_feedback.emit(super::sound::Cue::Focus);
        }
        self.set_selection(index);
    }

    fn set_selection(&mut self, index: usize) {
        if self.selected != index {
            self.cancel_hold();
            if self.screen == Screen::Favorites {
                self.shelf_age = 0;
            } else {
                self.catalog_age = 0;
            }
        }
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
            self.open(self.catalog_screen);
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
        #[cfg(feature = "audio")]
        if self.overlay.is_some() {
            self.audio_feedback.emit(super::sound::Cue::Back);
        }
        self.overlay = None;
    }
    pub fn select_action(&mut self, index: usize) {
        let index = index.min(self.item_count().saturating_sub(1));
        if self.menu_selection != index {
            self.menu_age = 0;
            if matches!(self.overlay, Some(Overlay::Details(_))) {
                self.details.expanded = false;
                self.details.scroll = 0;
                self.detail_revision = self.detail_revision.wrapping_add(1);
            }
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Focus);
        }
        self.menu_selection = index;
        self.update_detail_offset();
    }

    pub fn show_game_details(&mut self) {
        let game = self
            .games
            .iter()
            .find(|game| Some(game.key()) == self.selected_game)
            .map(|game| (game.key(), game.name(), game.platform()));
        if let Some((key, title, platform)) = game {
            self.details.expanded = false;
            self.details.scroll = 0;
            self.details.load = super::details::Load::Ready {
                document: super::details::Document::example(
                    key,
                    title,
                    platform
                        .and_then(|index| PLATFORMS.get(index))
                        .map_or("", |value| value.0),
                ),
            };
            self.show_overlay(Overlay::Details(super::details::Section::Overview));
            self.prepare_detail_geometry();
            self.detail_revision = self.detail_revision.wrapping_add(1);
        }
    }

    pub fn detail_section(&mut self, section: super::details::Section) {
        if !matches!(self.overlay, Some(Overlay::Details(_))) {
            return;
        }
        self.details.expanded = false;
        self.details.scroll = 0;
        self.show_overlay(Overlay::Details(section));
        self.prepare_detail_geometry();
        self.detail_revision = self.detail_revision.wrapping_add(1);
    }

    pub fn toggle_detail(&mut self) {
        if matches!(self.overlay, Some(Overlay::Details(section)) if section != super::details::Section::Overview)
            && self.menu_selection < self.item_count()
        {
            self.details.expanded = !self.details.expanded;
            self.details.scroll = 0;
            self.detail_revision = self.detail_revision.wrapping_add(1);
            self.update_detail_offset();
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
    }

    pub fn confirm_detail(&mut self) {
        use super::details::Section;
        match self.read_item(self.menu_selection).map(|item| item.action) {
            Some(MenuAction::ShowCoreDetails) => self.detail_section(Section::Core),
            Some(MenuAction::ShowFirmwareDetails) => self.detail_section(Section::Firmware),
            Some(MenuAction::ShowSaveDetails) => self.detail_section(Section::Saves),
            Some(MenuAction::ExpandDetail) => self.toggle_detail(),
            _ => {}
        }
    }

    pub fn move_detail(&mut self, direction: isize, vertical: bool) {
        if !matches!(self.overlay, Some(Overlay::Details(_))) {
            return;
        }
        if self.details.expanded && vertical {
            self.details.scroll = self
                .details
                .scroll
                .saturating_add_signed(direction.signum() as i32 * 24)
                .min(8192);
        } else {
            self.select_action(self.menu_selection.saturating_add_signed(direction));
            self.details.expanded = false;
            self.details.scroll = 0;
        }
        self.detail_revision = self.detail_revision.wrapping_add(1);
        self.update_detail_offset();
    }

    pub fn set_detail_load(&mut self, key: u32, load: super::details::Load) {
        if !matches!(self.overlay, Some(Overlay::Details(_)))
            || self.selected_game != Some(key)
            || matches!(&load, super::details::Load::Ready { document } if document.key != key)
        {
            return;
        }
        self.details.load = load;
        self.prepare_detail_geometry();
        self.details.expanded = false;
        self.details.scroll = 0;
        self.menu_selection = self.menu_selection.min(self.item_count().saturating_sub(1));
        self.detail_revision = self.detail_revision.wrapping_add(1);
    }

    fn read_detail_heading(&self) -> String {
        match &self.details.load {
            super::details::Load::Ready { document } => document.title.clone(),
            super::details::Load::Failed { message } => alloc::format!("读取未完成：{message}"),
            super::details::Load::Loading => "正在读取游戏信息…".into(),
        }
    }
    fn read_detail_platform(&self) -> String {
        match &self.details.load {
            super::details::Load::Ready { document } => document.platform.into(),
            _ => String::new(),
        }
    }
    fn read_detail_expanded(&self) -> bool {
        self.details.expanded
    }

    fn detail_section_value(&self) -> super::details::Section {
        match self.overlay {
            Some(Overlay::Details(section)) => section,
            _ => super::details::Section::Overview,
        }
    }
    fn prepare_detail_geometry(&mut self) {
        self.details.prepare_geometry();
        self.detail_geometry = self.detail_geometry.wrapping_add(1);
        self.detail_offset = Fixed::ZERO;
    }
    fn update_detail_offset(&mut self) {
        if matches!(self.overlay, Some(Overlay::Details(_))) {
            self.detail_offset =
                self.details
                    .offset(self.menu_selection, self.item_count(), self.detail_offset);
        }
    }
    pub fn detail_viewport(&mut self, height: Fixed) {
        self.details.viewport(height);
        self.update_detail_offset();
    }
    pub fn detail_measure(&mut self, index: usize, height: Fixed) {
        self.details.measure(index, height);
        self.update_detail_offset();
    }
    pub fn detail_text_size(&mut self, index: usize, part: usize, height: Fixed) {
        if self.details.text_size(index, part, height) {
            self.detail_geometry = self.detail_geometry.wrapping_add(1);
        }
    }
    pub fn detail_fact_measure(&mut self, index: usize, field: usize, value: bool, height: Fixed) {
        if self.details.fact_measure(index, field, value, height) {
            self.detail_geometry = self.detail_geometry.wrapping_add(1);
        }
    }
    fn read_detail_fact(&self, index: usize, field: usize) -> super::details::Fact {
        self.details
            .rows(self.detail_section_value())
            .get(index)
            .and_then(|row| row.facts.get(field))
            .copied()
            .unwrap_or_default()
    }
    fn read_detail_fact_count(&self, index: usize) -> usize {
        self.details
            .rows(self.detail_section_value())
            .get(index)
            .map_or(0, |row| row.facts.len())
    }
    fn read_detail_fact_height(&self, index: usize, field: usize) -> Fixed {
        self.details.fact_height(index, field)
    }
    fn read_detail_facts_size(&self, index: usize, gap: i32) -> Fixed {
        self.details
            .facts_size(self.detail_section_value(), index, gap)
    }
    fn read_detail_size(&self, index: usize, gap: i32) -> Fixed {
        self.details
            .size(self.detail_section_value(), self.menu_selection, index, gap)
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.query_revision = self.query_revision.wrapping_add(1);
        self.keyboard = false;
        self.select_result();
    }

    pub fn type_key(&mut self, key: &'static str) {
        #[cfg(feature = "audio")]
        self.audio_feedback.emit(super::sound::Cue::Confirm);
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
        self.set_selection(key);
    }

    pub fn edit_query(&mut self) {
        #[cfg(feature = "audio")]
        if !self.keyboard {
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
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
            self.toggle_game_favorite(key);
            self.overlay = None;
        }
    }
    pub fn toggle_game_favorite(&mut self, key: u32) {
        let mut changed = false;
        for game in self.games.iter_mut() {
            if game.key() == key {
                game.toggle_favorite();
                changed = true;
            }
        }
        if changed {
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
    }

    pub fn launch(&mut self) {
        if self.connected && self.overlay != Some(Overlay::Network) {
            self.show_overlay(Overlay::Network);
            return;
        }
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
        #[cfg(feature = "audio")]
        self.audio_feedback.emit(super::sound::Cue::Start);
    }

    pub fn network(&mut self) {
        #[cfg(feature = "audio")]
        self.audio_feedback.emit(super::sound::Cue::Confirm);
        self.connected = !self.connected;
        self.notice = if self.connected {
            "已连接 · LOCAL DEMO"
        } else {
            "已断开 · LOCAL DEMO"
        };
    }
    pub fn retain_network(&mut self) {
        self.connected = true;
    }
    pub fn clear_network(&mut self) {
        self.connected = false;
    }

    pub fn toggle_sound(&mut self) {
        #[cfg(feature = "audio")]
        {
            let (sound, volume) = self.audio_feedback.preferences(self.sound, self.volume);
            self.sound = !sound;
            self.volume = volume;
            self.audio_feedback.configure(self.sound, self.volume);
        }
        #[cfg(not(feature = "audio"))]
        {
            self.sound = !self.sound;
        }
    }

    fn read_level(&self, index: usize) -> Option<Level> {
        let control = *SystemLevel::ALL.get(index)?;
        #[cfg(feature = "audio")]
        let (sound, volume) = self.audio_feedback.preferences(self.sound, self.volume);
        #[cfg(not(feature = "audio"))]
        let (sound, volume) = (self.sound, self.volume);
        Some(Level {
            value: (self.level_status[index] != LevelStatus::Reading).then_some(match control {
                SystemLevel::Volume => volume,
                SystemLevel::Brightness => self.brightness,
            }),
            minimum: control.minimum(),
            maximum: 100,
            step: 5,
            muted: control == SystemLevel::Volume && !sound,
            pending: self.level_status[index] == LevelStatus::Pending,
        })
    }

    pub fn set_level_status(&mut self, control: SystemLevel, status: LevelStatus) {
        self.level_status[control.index()] = status;
        if status == LevelStatus::Reading {
            self.level_motion[control.index()] =
                Tween::preset(Fixed::ZERO, Fixed::ZERO, LEVEL_MOTION);
            self.level_positions[control.index()] = Fixed::ZERO;
        }
    }

    pub fn set_level(&mut self, control: SystemLevel, value: u8) {
        #[cfg(feature = "audio")]
        {
            (self.sound, self.volume) = self.audio_feedback.preferences(self.sound, self.volume);
        }
        if matches!(
            self.level_status[control.index()],
            LevelStatus::Reading | LevelStatus::Unavailable(_)
        ) {
            return;
        }
        let value = value.clamp(control.minimum(), 100);
        let current = match control {
            SystemLevel::Volume => &mut self.volume,
            SystemLevel::Brightness => &mut self.brightness,
        };
        if *current == value {
            return;
        }
        *current = value;
        self.level_motion[control.index()].retarget(i32::from(value).into(), LEVEL_MOTION);
        #[cfg(feature = "audio")]
        if control == SystemLevel::Volume {
            self.audio_feedback.configure(self.sound, self.volume);
        } else {
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
    }

    pub fn adjust_level(&mut self, control: SystemLevel, direction: isize) {
        #[cfg(feature = "audio")]
        {
            (self.sound, self.volume) = self.audio_feedback.preferences(self.sound, self.volume);
        }
        let value = match control {
            SystemLevel::Volume => self.volume,
            SystemLevel::Brightness => self.brightness,
        };
        let next = (isize::from(value) + direction.signum() * 5)
            .clamp(isize::from(control.minimum()), 100) as u8;
        self.set_level(control, next);
    }

    pub fn adjust(&mut self, tool: MenuAction) {
        #[cfg(feature = "audio")]
        {
            (self.sound, self.volume) = self.audio_feedback.preferences(self.sound, self.volume);
        }
        let old = (self.sound, self.volume, self.brightness, self.palette);
        match tool {
            MenuAction::ToggleSound => self.toggle_sound(),
            MenuAction::CycleTheme => self.palette = (self.palette + 1) % 3,
            _ => return,
        }
        if old != (self.sound, self.volume, self.brightness, self.palette) {
            #[cfg(feature = "audio")]
            {
                self.audio_feedback.emit(super::sound::Cue::Confirm);
            }
        }
        self.notice = "LOCAL DEMO: settings updated";
        #[cfg(feature = "audio")]
        if matches!(tool, MenuAction::ToggleSound) {
            self.notice = "";
        }
    }

    pub fn result(&mut self, notice: &'static str) {
        self.notice = notice;
    }

    pub fn begin_hold(&mut self, key: u32) {
        if !matches!(self.gesture, Gesture::Idle) {
            return;
        }
        if self.active
            && self.overlay.is_none()
            && matches!(
                self.screen,
                Screen::Favorites | Screen::Games | Screen::Search
            )
        {
            self.gesture = Gesture::Pressed { key, elapsed: 0 };
            self.hold_progress = Fixed::ZERO;
        } else {
            self.gesture = Gesture::Consumed;
        }
    }

    pub fn advance_hold(&mut self, dt: u16) {
        if dt > MAX_FRAME_GAP_MS {
            self.cancel_hold();
            return;
        }
        let Gesture::Pressed { key, elapsed } = &mut self.gesture else {
            return;
        };
        *elapsed = elapsed.saturating_add(dt);
        if *elapsed >= HOLD_MS {
            let key = *key;
            self.gesture = Gesture::Consumed;
            self.hold_progress = Fixed::ZERO;
            self.pending_play = Some((key, true));
            return;
        }
        if *elapsed >= FEEDBACK_MS {
            self.hold_progress = Fixed::from_ratio(i32::from(*elapsed), i32::from(HOLD_MS));
        }
    }

    pub fn release_hold(&mut self) {
        self.hold_progress = Fixed::ZERO;
        if let Gesture::Pressed { key, .. } = core::mem::take(&mut self.gesture) {
            self.pending_play = Some((key, false));
        }
    }
    pub fn pending_play(&self) -> Option<(u32, bool)> {
        self.pending_play
    }
    pub fn acknowledge_play(&mut self) {
        self.pending_play = None;
    }

    pub fn cancel_hold(&mut self) {
        if !matches!(self.gesture, Gesture::Idle) {
            self.gesture = Gesture::Consumed;
        }
        self.hold_progress = Fixed::ZERO;
    }

    pub fn holding(&self) -> bool {
        matches!(self.gesture, Gesture::Pressed { .. })
    }

    #[observe]
    pub fn menu(&self) -> bool {
        self.overlay.is_some()
    }
    #[observe]
    pub fn item_count(&self) -> usize {
        match self.overlay {
            Some(Overlay::Details(section)) if section != super::details::Section::Overview => {
                self.details.rows(section).len()
            }
            _ => self.overlay.map_or(0, |overlay| overlay.items().len()),
        }
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
            Some(Overlay::Details(super::details::Section::Overview)) => "游戏详情".into(),
            Some(Overlay::Details(super::details::Section::Core)) => "运行核心".into(),
            Some(Overlay::Details(super::details::Section::Firmware)) => "BIOS 文件".into(),
            Some(Overlay::Details(super::details::Section::Saves)) => "存档与配置".into(),
            None => String::new(),
        }
    }
    fn read_item(&self, index: usize) -> Option<MenuItem> {
        use MenuAction::*;
        if let Some(Overlay::Details(section)) = self.overlay
            && section != super::details::Section::Overview
        {
            let row = self.details.rows(section).get(index)?;
            return Some(MenuItem {
                action: ExpandDetail,
                name: row.name,
                value: row.summary.into(),
                enabled: true,
                reason: "",
                level: None,
            });
        }
        let (action, mut name) = *self.overlay?.items().get(index)?;
        let mut enabled = true;
        let mut reason = "";
        let mut value = String::new();
        #[cfg(feature = "audio")]
        let (sound, _volume) = if matches!(action, ToggleSound | Volume) {
            self.audio_feedback.preferences(self.sound, self.volume)
        } else {
            (self.sound, self.volume)
        };
        #[cfg(not(feature = "audio"))]
        let (sound, _volume) = (self.sound, self.volume);
        let level = match action {
            Volume => self.read_level(0),
            Brightness => self.read_level(1),
            _ => None,
        };
        match action {
            ShowCoreDetails => value = "共享选择规则与安装来源".into(),
            ShowFirmwareDetails => value = "固件声明与文件状态".into(),
            ShowSaveDetails => value = "单机来源与联机隔离".into(),
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
            Volume | Brightness => {
                let index = if action == Volume { 0 } else { 1 };
                match self.level_status[index] {
                    LevelStatus::Reading => {
                        enabled = false;
                        reason = "正在读取系统设置";
                    }
                    LevelStatus::Unavailable(message) => {
                        enabled = false;
                        reason = message;
                    }
                    _ => {}
                }
            }
            ToggleSound => value = if sound { "开".into() } else { "关".into() },
            CycleTheme => value = ["森林", "落日", "夜色"][self.palette].into(),
            _ => {}
        }
        Some(MenuItem {
            action,
            name,
            enabled,
            reason,
            value,
            level,
        })
    }
    pub fn show_overlay(&mut self, overlay: Overlay) {
        #[cfg(feature = "audio")]
        if self.overlay != Some(overlay) {
            self.audio_feedback.emit(super::sound::Cue::Confirm);
        }
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
                Overlay::Details(section) => {
                    if self.details.expanded {
                        self.details.expanded = false;
                        self.details.scroll = 0;
                    } else if section == super::details::Section::Overview {
                        self.show_overlay(Overlay::Game);
                        self.select_action(3);
                        self.details.load = super::details::Load::Loading;
                    } else {
                        self.show_overlay(Overlay::Details(super::details::Section::Overview));
                        self.select_action(match section {
                            super::details::Section::Core => 0,
                            super::details::Section::Firmware => 1,
                            _ => 2,
                        });
                    }
                    self.detail_revision = self.detail_revision.wrapping_add(1);
                    if self.overlay != Some(overlay) {
                        self.prepare_detail_geometry();
                    }
                    self.update_detail_offset();
                }
                Overlay::Tools => {
                    self.show_overlay(Overlay::Controls);
                    self.select_action(2);
                }
                Overlay::Controls => {
                    self.show_overlay(Overlay::System);
                    self.select_action(2);
                }
                _ => self.overlay = None,
            }
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Back);
            return;
        }
        if self.screen == Screen::Search && self.keyboard {
            self.keyboard = false;
            self.selected = 0;
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Back);
            return;
        }
        if self.screen == Screen::Favorites {
            let first = self
                .games
                .iter()
                .find(|game| game.favorite())
                .map(|game| game.key() as usize);
            if let Some(first) = first {
                self.select(first);
            }
            #[cfg(feature = "audio")]
            self.audio_feedback.emit(super::sound::Cue::Back);
            return;
        }
        let target = if self.screen == Screen::Games {
            self.origin
        } else {
            Screen::Favorites
        };
        self.open(target);
        if target == Screen::Platforms {
            self.set_selection(self.platform);
        }
        self.direction = PageDirection::Backward;
        #[cfg(feature = "audio")]
        self.audio_feedback.emit(super::sound::Cue::Back);
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
    pub fn detail_active(&self) -> bool {
        matches!(self.overlay(), Some(Overlay::Details(_)))
    }
    pub fn detail_count(&self) -> usize {
        if self.detail_active() {
            self.item_count()
        } else {
            0
        }
    }
    pub fn detail_fact_count(&self, index: usize) -> usize {
        self.detail_revision();
        self.read_detail_fact_count(index)
    }
    pub fn detail_fact(&self, index: usize, field: usize) -> super::details::Fact {
        self.detail_revision();
        self.read_detail_fact(index, field)
    }
    pub fn detail_fact_height(&self, index: usize, field: usize) -> Fixed {
        self.detail_geometry();
        self.read_detail_fact_height(index, field)
    }
    pub fn detail_facts_size(&self, index: usize, gap: i32) -> Fixed {
        self.detail_revision();
        self.detail_geometry();
        self.read_detail_facts_size(index, gap)
    }
    pub fn detail_size(&self, index: usize, gap: i32) -> Fixed {
        self.detail_revision();
        self.detail_geometry();
        self.menu_selection();
        self.read_detail_size(index, gap)
    }
    pub fn detail_heading(&self) -> String {
        self.detail_revision();
        self.read_detail_heading()
    }
    pub fn detail_platform(&self) -> String {
        self.detail_revision();
        self.read_detail_platform()
    }
    pub fn detail_expanded(&self) -> bool {
        self.detail_revision();
        self.detail_active() && self.read_detail_expanded()
    }
    pub fn level(&self, index: usize) -> Option<Level> {
        self.volume();
        self.brightness();
        self.sound();
        self.level_status();
        self.read_level(index)
    }
    pub fn query(&self) -> String {
        self.query_revision();
        self.query_value()
    }

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
                .any(|game| game.platform() == Some(self.platform())),
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
            Screen::Search if self.keyboard() => "Y 退格 · K 结果",
            Screen::Search => {
                let query = self.query().to_lowercase();
                if !query.trim().is_empty()
                    && self
                        .games()
                        .iter()
                        .any(|game| game.key() as usize == self.selected() && game.matches(&query))
                {
                    "X 游玩 · Y 收藏 · K 键盘"
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
        self.detail_revision();
        self.overlay();
        self.sound();
        self.connected();
        self.volume();
        self.brightness();
        self.level_status();
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
    fn detail_navigation_preserves_game_and_restores_each_parent_action() {
        use super::super::details::Section;
        let mut state = Launcher::default();
        state.choose_game(1, 1);
        assert_eq!(
            state.read_item(3).unwrap().action,
            MenuAction::ShowGameDetails
        );
        assert_eq!(state.read_item(4).unwrap().action, MenuAction::Back);
        state.show_game_details();
        assert_eq!(state.item_count(), 3);
        for (section, parent) in [
            (Section::Core, 0),
            (Section::Firmware, 1),
            (Section::Saves, 2),
        ] {
            state.detail_section(section);
            assert!(state.item_count() > 0);
            state.toggle_detail();
            assert!(state.details.expanded);
            state.move_detail(1, true);
            assert_eq!(state.details.scroll, 24);
            let current = state.menu_selection;
            state.back();
            assert_eq!(state.overlay, Some(Overlay::Details(section)));
            assert!(!state.details.expanded);
            assert_eq!(state.menu_selection, current);
            state.back();
            assert_eq!(state.overlay, Some(Overlay::Details(Section::Overview)));
            assert_eq!(state.menu_selection, parent);
        }
        state.back();
        assert_eq!(state.overlay, Some(Overlay::Game));
        assert_eq!(state.menu_selection, 3);
        state.back();
        assert!(!state.menu());
        assert_eq!((state.selected, state.selected_game), (1, Some(1)));
    }

    #[test]
    fn detail_examples_distinguish_status_and_reject_unrelated_games() {
        use super::super::details::{FileStatus, Load, Requirement, Section};
        let mut state = Launcher::default();
        state.choose_game(0, 0);
        state.show_game_details();
        state.detail_section(Section::Firmware);
        let row = state.details.rows(Section::Firmware)[0];
        assert_eq!(row.status, FileStatus::Missing);
        assert_eq!(row.requirement, Some(Requirement::Optional));
        assert!(
            row.facts
                .iter()
                .all(|fact| !fact.label.is_empty() && !fact.value.is_empty())
        );
        state.set_detail_load(1, Load::Loading);
        assert_eq!(state.item_count(), 1);
        state.set_detail_load(0, Load::Loading);
        assert_eq!(state.item_count(), 0);
        assert_eq!(state.read_detail_heading(), "正在读取游戏信息…");
        state.set_detail_load(
            0,
            Load::Failed {
                message: "读取失败",
            },
        );
        assert!(state.read_detail_heading().contains("读取失败"));
        state.close();
        state.set_detail_load(0, Load::Loading);
        assert!(matches!(state.details.load, Load::Failed { .. }));
    }

    #[test]
    fn expanded_detail_scroll_and_horizontal_navigation_have_separate_ownership() {
        use super::super::details::Section;
        let mut state = Launcher::default();
        state.choose_game(0, 0);
        state.show_game_details();
        state.detail_section(Section::Saves);
        state.toggle_detail();
        for _ in 0..400 {
            state.move_detail(1, true);
        }
        assert_eq!(state.details.scroll, 8192);
        assert_eq!(state.menu_selection, 0);
        state.move_detail(1, false);
        assert_eq!(state.menu_selection, 1);
        assert!(!state.details.expanded);
        assert_eq!(state.details.scroll, 0);
    }
    #[test]
    fn simulated_levels_keep_confirmed_values_bounds_and_parent_focus() {
        let mut state = Launcher::default();
        state.show_overlay(Overlay::Controls);
        assert_eq!(state.item_count(), 4);
        state.adjust_level(SystemLevel::Volume, -1);
        assert_eq!(state.volume, 55);
        for _ in 0..30 {
            state.adjust_level(SystemLevel::Brightness, -1);
        }
        assert_eq!(state.brightness, 1);
        state.set_level_status(SystemLevel::Volume, LevelStatus::Reading);
        assert_eq!(state.read_level(0).unwrap().value, None);
        assert!(!state.read_item(0).unwrap().enabled);
        state.adjust_level(SystemLevel::Volume, 1);
        assert_eq!(state.volume, 55);
        state.set_level_status(
            SystemLevel::Volume,
            LevelStatus::Unavailable("设置读取失败"),
        );
        assert_eq!(state.read_level(0).unwrap().value, Some(55));
        assert!(!state.read_item(0).unwrap().enabled);
        state.set_level_status(SystemLevel::Volume, LevelStatus::Pending);
        assert!(state.read_level(0).unwrap().pending);
        assert!(state.read_item(0).unwrap().enabled);
        state.show_overlay(Overlay::Tools);
        state.back();
        assert_eq!(state.overlay, Some(Overlay::Controls));
        assert_eq!(state.menu_selection, 2);
        state.back();
        assert_eq!(state.overlay, Some(Overlay::System));
        assert_eq!(state.menu_selection, 2);
        assert_eq!(state.volume, 55);
    }

    #[test]
    fn level_tweens_redirect_from_the_displayed_value() {
        let mut state = Launcher::default();
        state.set_level(SystemLevel::Volume, 90);
        state.animate(75);
        let middle = state.level_positions[0];
        assert!(middle > 60.into() && middle < 90.into());
        state.set_level(SystemLevel::Volume, 30);
        assert_eq!(state.level_positions[0], middle);
        state.animate(150);
        assert_eq!(state.level_positions[0], 30.into());
        state.set_level(SystemLevel::Volume, 30);
        assert!(state.level_motion[0].is_finished());
    }

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
        assert_eq!(launcher.menu_selection, 2);
        launcher.back();
        assert_eq!(launcher.overlay, Some(Overlay::System));
        assert_eq!(launcher.menu_selection, 2);
        launcher.back();
        assert_eq!(launcher.overlay, None);
        assert_eq!((launcher.screen, launcher.selected), (Screen::Games, 3));
        launcher.show_overlay(Overlay::Power);
        launcher.back();
        assert_eq!(launcher.overlay, None);
        launcher.show_overlay(Overlay::System);
        assert_eq!(launcher.item_count(), 6);
        assert!(!launcher.read_item(3).unwrap().enabled);
        assert!(!launcher.read_item(3).unwrap().reason.is_empty());
    }
    #[test]
    fn game_overlay_keeps_existing_actions_and_adds_details_before_back() {
        let mut launcher = Launcher::default();
        launcher.choose_game(2, 2);
        assert_eq!(launcher.item_count(), 5);
        assert!(!launcher.read_item(1).unwrap().enabled);
        launcher.back();
        assert!(!launcher.menu());
        assert_eq!(launcher.selected_game, Some(2));
        launcher.close();
        launcher.resume();
        assert!(launcher.active);
    }
}
