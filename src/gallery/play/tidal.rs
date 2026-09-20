use crate::gallery::play::change::ChangeSet;

pub(crate) const BOARD_SIZE: usize = 36;
pub(crate) const ISLAND_COUNT: usize = 4;
pub(crate) const HISTORY_CAPACITY: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Tile {
    #[default]
    Sea = 0,
    Grove = 1,
    Field = 2,
    Hamlet = 3,
    Harbor = 4,
    Lens = 5,
    Lagoon = 6,
    Beacon = 7,
    Dike = 8,
}

impl Tile {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Sea => "海域",
            Self::Grove => "森林",
            Self::Field => "梯田",
            Self::Hamlet => "聚落",
            Self::Harbor => "港湾",
            Self::Lens => "观星台",
            Self::Lagoon => "泻湖",
            Self::Beacon => "灯塔",
            Self::Dike => "石堤",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Sea => "相邻海域",
            Self::Grove => "基础 2；每片邻林 +2",
            Self::Field => "基础 2；每面临水 +2",
            Self::Hamlet => "基础 3；每种邻居 +2",
            Self::Harbor => "基础 1；每面临水 +2",
            Self::Lens => "基础 3；每面空海 +2",
            Self::Lagoon => "基础 1；邻田 / 港 +2",
            Self::Beacon => "基础 2；每座邻港 +3",
            Self::Dike => "基础 2；每片邻林 +1",
        }
    }

    pub(crate) const fn rule(self) -> &'static str {
        match self {
            Self::Sea => "海域为相邻地块提供水面",
            Self::Grove => "邻接梯田再 +1",
            Self::Field | Self::Hamlet => "涨潮时，低地停产",
            Self::Harbor => "涨潮时额外 +3",
            Self::Lens => "长夜时额外 +3",
            Self::Lagoon => "为邻格永久提供水面",
            Self::Beacon => "风暴时额外 +2",
            Self::Dike => "保护四邻低地不被淹",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TideLevel {
    #[default]
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Weather {
    #[default]
    Clear,
    Harvest,
    LongNight,
    Storm,
}

impl Weather {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Clear => "晴朗",
            Self::Harvest => "丰收",
            Self::LongNight => "长夜",
            Self::Storm => "风暴",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Forecast {
    pub(crate) tide: TideLevel,
    pub(crate) weather: Weather,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Perk {
    Forest,
    Water,
    Town,
    Star,
    Levee,
    Lagoon,
    Beacon,
    Survey,
}

impl Perk {
    const ALL: [Self; 8] = [
        Self::Forest,
        Self::Water,
        Self::Town,
        Self::Star,
        Self::Levee,
        Self::Lagoon,
        Self::Beacon,
        Self::Survey,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Forest => "林间协议",
            Self::Water => "潮汐学说",
            Self::Town => "邻里公约",
            Self::Star => "长夜观测",
            Self::Levee => "低地复兴",
            Self::Lagoon => "水脉复苏",
            Self::Beacon => "远航信标",
            Self::Survey => "高地测绘",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Forest => "森林每次结算 +2",
            Self::Water => "港湾每次结算 +2",
            Self::Town => "聚落每种邻居再 +1",
            Self::Star => "观星台每次结算 +3",
            Self::Levee => "梯田 / 聚落免疫涨潮",
            Self::Lagoon => "泻湖每次结算 +3",
            Self::Beacon => "灯塔每次结算 +3",
            Self::Survey => "高地建筑每次结算 +1",
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Goal {
    pub(crate) name: &'static str,
    pub(crate) tile: Tile,
    pub(crate) count: u8,
}

const GOALS: [Goal; 6] = [
    Goal {
        name: "连片森林",
        tile: Tile::Grove,
        count: 5,
    },
    Goal {
        name: "潮汐农场",
        tile: Tile::Field,
        count: 4,
    },
    Goal {
        name: "港口之约",
        tile: Tile::Harbor,
        count: 4,
    },
    Goal {
        name: "星图编织",
        tile: Tile::Lens,
        count: 3,
    },
    Goal {
        name: "邻里计划",
        tile: Tile::Hamlet,
        count: 4,
    },
    Goal {
        name: "万象群岛",
        tile: Tile::Sea,
        count: 8,
    },
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct IslandResult {
    pub(crate) score: u16,
    pub(crate) target: u16,
    pub(crate) goal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TideModal {
    #[default]
    None,
    Voyage,
    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TideMessage {
    Ready,
    Placed(Tile),
    Harvest(u8, u16),
    Rerolled,
    Undone,
    Settled(bool),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rng(u32);

impl Rng {
    const fn new(seed: u32) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.0 = value;
        value
    }

    fn index(&mut self, len: u32) -> usize {
        ((u64::from(self.next()) * u64::from(len)) >> 32) as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TideState {
    terrain: [u8; BOARD_SIZE],
    board: [Tile; BOARD_SIZE],
    forecasts: [Forecast; 4],
    offer: [Tile; 3],
    perk_offer: [Perk; 3],
    harvests: [u16; 4],
    results: [IslandResult; ISLAND_COUNT],
    rng: u32,
    total: u16,
    score: u16,
    target: u16,
    perks: u8,
    discovered: u16,
    chapter: u8,
    turn: u8,
    rerolls: u8,
    harvest_len: u8,
    result_len: u8,
    goal: u8,
    settled: bool,
    complete: bool,
}

impl TideState {
    const EMPTY: Self = Self {
        terrain: [0; BOARD_SIZE],
        board: [Tile::Sea; BOARD_SIZE],
        forecasts: [Forecast {
            tide: TideLevel::Low,
            weather: Weather::Clear,
        }; 4],
        offer: [Tile::Grove; 3],
        perk_offer: [Perk::Forest; 3],
        harvests: [0; 4],
        results: [IslandResult {
            score: 0,
            target: 0,
            goal: false,
        }; ISLAND_COUNT],
        rng: 1,
        total: 0,
        score: 0,
        target: 430,
        perks: 0,
        discovered: 0,
        chapter: 0,
        turn: 0,
        rerolls: 3,
        harvest_len: 0,
        result_len: 0,
        goal: 0,
        settled: false,
        complete: false,
    };
}

#[derive(Clone, Copy, Debug)]
struct TideHistory {
    entries: [TideState; HISTORY_CAPACITY],
    start: u8,
    len: u8,
}

impl TideHistory {
    const fn new() -> Self {
        Self {
            entries: [TideState::EMPTY; HISTORY_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.start = 0;
        self.len = 0;
    }

    fn push(&mut self, state: TideState) {
        if usize::from(self.len) == HISTORY_CAPACITY {
            self.entries[usize::from(self.start)] = state;
            self.start = (self.start + 1) % HISTORY_CAPACITY as u8;
        } else {
            let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
            self.entries[usize::from(index)] = state;
            self.len += 1;
        }
    }

    fn pop(&mut self) -> Option<TideState> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.entries[usize::from((self.start + self.len) % HISTORY_CAPACITY as u8)])
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TideModel {
    state: TideState,
    history: TideHistory,
    seed: u32,
    selected: Option<u8>,
    pending: Option<u8>,
    choice: u8,
    modal: TideModal,
    message: TideMessage,
}

impl Default for TideModel {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl TideModel {
    pub(crate) fn new(seed: u32) -> Self {
        let mut model = Self {
            state: TideState::EMPTY,
            history: TideHistory::new(),
            seed: seed.max(1),
            selected: None,
            pending: None,
            choice: 0,
            modal: TideModal::None,
            message: TideMessage::Ready,
        };
        model.make_island();
        model
    }

    fn make_island(&mut self) {
        let persistent = self.state;
        let mut state = TideState::EMPTY;
        state.chapter = persistent.chapter;
        state.total = persistent.total;
        state.perks = persistent.perks;
        state.discovered = if persistent.discovered == 0 {
            0b0110_0010
        } else {
            persistent.discovered
        };
        state.results = persistent.results;
        state.result_len = persistent.result_len;
        state.target = 430 + u16::from(state.chapter) * 55;
        let island_seed = self
            .seed
            .wrapping_add(u32::from(state.chapter + 1).wrapping_mul(2_654_435_761));
        let mut rng = Rng::new(island_seed);
        for terrain in &mut state.terrain {
            *terrain = rng.index(3) as u8;
        }
        state.board[14] = Tile::Beacon;
        state.board[15] = Tile::Grove;
        state.board[20] = Tile::Lagoon;
        state.terrain[14] = 2;
        for (index, forecast) in state.forecasts.iter_mut().enumerate() {
            forecast.tide = if index % 2 == 1 {
                TideLevel::High
            } else {
                TideLevel::Low
            };
            forecast.weather = match rng.index(4) {
                0 => Weather::Clear,
                1 => Weather::Harvest,
                2 => Weather::LongNight,
                _ => Weather::Storm,
            };
        }
        state.goal = rng.index(GOALS.len() as u32) as u8;
        state.rng = rng.0;
        self.state = state;
        self.refill();
        self.history.clear();
        self.selected = None;
        self.pending = None;
        self.choice = 0;
        self.modal = TideModal::None;
        self.message = TideMessage::Ready;
    }

    fn refill(&mut self) {
        let mut rng = Rng::new(self.state.rng);
        let mut tiles = [
            Tile::Grove,
            Tile::Field,
            Tile::Hamlet,
            Tile::Harbor,
            Tile::Lens,
            Tile::Lagoon,
            Tile::Beacon,
            Tile::Dike,
        ];
        for index in (1..tiles.len()).rev() {
            let other = rng.index((index + 1) as u32);
            tiles.swap(index, other);
        }
        self.state.offer.copy_from_slice(&tiles[..3]);
        self.state.rng = rng.0;
    }

    #[cfg(test)]
    pub(crate) const fn seed(&self) -> u32 {
        self.seed
    }
    pub(crate) const fn chapter(&self) -> u8 {
        self.state.chapter
    }
    pub(crate) const fn turn(&self) -> u8 {
        self.state.turn
    }
    pub(crate) const fn score(&self) -> u16 {
        self.state.score
    }
    #[cfg(test)]
    pub(crate) const fn total(&self) -> u16 {
        self.state.total
    }
    pub(crate) const fn target(&self) -> u16 {
        self.state.target
    }
    pub(crate) const fn rerolls(&self) -> u8 {
        self.state.rerolls
    }
    pub(crate) const fn settled(&self) -> bool {
        self.state.settled
    }
    pub(crate) const fn complete(&self) -> bool {
        self.state.complete
    }
    pub(crate) const fn history_len(&self) -> u8 {
        self.history.len
    }
    pub(crate) const fn selected(&self) -> Option<u8> {
        self.selected
    }
    pub(crate) const fn pending(&self) -> Option<u8> {
        self.pending
    }
    pub(crate) const fn choice(&self) -> u8 {
        self.choice
    }
    pub(crate) const fn modal(&self) -> TideModal {
        self.modal
    }
    pub(crate) const fn message(&self) -> TideMessage {
        self.message
    }
    pub(crate) const fn tile(&self, index: usize) -> Tile {
        self.state.board[index]
    }
    pub(crate) const fn terrain(&self, index: usize) -> u8 {
        self.state.terrain[index]
    }
    pub(crate) const fn offer(&self, index: usize) -> Tile {
        self.state.offer[index]
    }
    pub(crate) const fn perk_offer(&self, index: usize) -> Perk {
        self.state.perk_offer[index]
    }
    pub(crate) const fn result(&self, index: usize) -> IslandResult {
        self.state.results[index]
    }
    pub(crate) const fn harvest(&self, index: usize) -> u16 {
        self.state.harvests[index]
    }
    pub(crate) const fn harvest_len(&self) -> u8 {
        self.state.harvest_len
    }
    pub(crate) const fn goal(&self) -> Goal {
        GOALS[self.state.goal as usize]
    }
    pub(crate) fn forecast(&self) -> Forecast {
        self.state.forecasts[self.season()]
    }
    pub(crate) fn season(&self) -> usize {
        core::cmp::min(3, self.state.turn as usize / 6)
    }

    pub(crate) fn cumulative_score(&self) -> u16 {
        self.state.total.saturating_add(self.state.score)
    }

    pub(crate) fn has_perk(&self, perk: Perk) -> bool {
        self.state.perks & perk.bit() != 0
    }

    pub(crate) fn goal_progress(&self) -> u8 {
        let goal = self.goal();
        if goal.tile != Tile::Sea {
            self.state
                .board
                .iter()
                .filter(|tile| **tile == goal.tile)
                .count() as u8
        } else {
            let mut found = 0_u16;
            for tile in self.state.board {
                if tile != Tile::Sea {
                    found |= 1 << tile as u8;
                }
            }
            found.count_ones() as u8
        }
    }

    pub(crate) fn valid(&self, index: usize) -> bool {
        index < BOARD_SIZE
            && self.state.board[index] == Tile::Sea
            && adjacent(index)
                .into_iter()
                .flatten()
                .any(|other| self.state.board[other] != Tile::Sea)
    }

    fn water(&self, index: usize) -> bool {
        matches!(self.state.board[index], Tile::Sea | Tile::Lagoon)
    }

    pub(crate) fn tile_score(&self, index: usize, forecast: Forecast) -> u16 {
        let tile = self.state.board[index];
        if tile == Tile::Sea {
            return 0;
        }
        let neighbours = adjacent(index);
        let count = |needle| {
            neighbours
                .into_iter()
                .flatten()
                .filter(|other| self.state.board[*other] == needle)
                .count() as u16
        };
        let water = neighbours
            .into_iter()
            .flatten()
            .filter(|other| self.water(*other))
            .count() as u16;
        let empty = neighbours
            .into_iter()
            .flatten()
            .filter(|other| self.state.board[*other] == Tile::Sea)
            .count() as u16;
        let flooded = self.state.terrain[index] == 0
            && forecast.tide == TideLevel::High
            && matches!(tile, Tile::Field | Tile::Hamlet)
            && count(Tile::Dike) == 0
            && !self.has_perk(Perk::Levee);
        if flooded {
            return 0;
        }
        let mut score = match tile {
            Tile::Grove => {
                2 + count(Tile::Grove) * 2
                    + u16::from(count(Tile::Field) != 0)
                    + 2 * u16::from(self.has_perk(Perk::Forest))
            }
            Tile::Field => {
                2 + water * 2
                    + count(Tile::Hamlet)
                    + 2 * u16::from(forecast.weather == Weather::Harvest)
            }
            Tile::Hamlet => {
                let mut kinds = 0_u16;
                for other in neighbours.into_iter().flatten() {
                    let value = self.state.board[other];
                    if value != Tile::Sea {
                        kinds |= 1 << value as u8;
                    }
                }
                3 + kinds.count_ones() as u16 * if self.has_perk(Perk::Town) { 3 } else { 2 }
            }
            Tile::Harbor => {
                1 + water * 2
                    + 3 * u16::from(forecast.tide == TideLevel::High)
                    + 2 * u16::from(self.has_perk(Perk::Water))
            }
            Tile::Lens => {
                3 + empty * 2
                    + 3 * u16::from(forecast.weather == Weather::LongNight)
                    + 3 * u16::from(self.has_perk(Perk::Star))
            }
            Tile::Lagoon => {
                1 + (count(Tile::Field) + count(Tile::Harbor)) * 2
                    + 3 * u16::from(self.has_perk(Perk::Lagoon))
            }
            Tile::Beacon => {
                2 + count(Tile::Harbor) * 3
                    + 2 * u16::from(forecast.weather == Weather::Storm)
                    + 3 * u16::from(self.has_perk(Perk::Beacon))
            }
            Tile::Dike => 2 + count(Tile::Grove),
            Tile::Sea => 0,
        };
        if self.has_perk(Perk::Survey) && self.state.terrain[index] == 2 {
            score += 1;
        }
        score
    }

    pub(crate) fn forecast_score(&self) -> u16 {
        let forecast = self.forecast();
        (0..BOARD_SIZE)
            .map(|index| self.tile_score(index, forecast))
            .sum()
    }

    pub(crate) fn preview(&self, index: usize, choice: usize) -> Option<(u16, i16)> {
        if !self.valid(index) || choice >= 3 {
            return None;
        }
        let before = self.forecast_score();
        let mut copy = *self;
        copy.state.board[index] = copy.state.offer[choice];
        let score = copy.tile_score(index, copy.forecast());
        let after = copy.forecast_score();
        Some((score, after as i16 - before as i16))
    }

    pub(crate) fn select_offer(&mut self, choice: u8) -> ChangeSet {
        if choice >= 3 || self.state.settled || self.state.complete {
            return ChangeSet::NONE;
        }
        self.choice = choice;
        self.selected = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_cell(&mut self, index: u8) -> ChangeSet {
        let index_usize = usize::from(index);
        if index_usize >= BOARD_SIZE {
            return ChangeSet::NONE;
        }
        if self.state.board[index_usize] != Tile::Sea {
            self.selected = Some(index);
            self.pending = None;
        } else if self.valid(index_usize) && !self.state.settled && !self.state.complete {
            self.pending = Some(index);
            self.selected = None;
        } else {
            return ChangeSet::NONE;
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn cancel_preview(&mut self) -> ChangeSet {
        if self.pending.take().is_none() {
            return ChangeSet::NONE;
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn place_pending(&mut self) -> ChangeSet {
        let Some(index) = self.pending else {
            return ChangeSet::NONE;
        };
        self.place(index, self.choice)
    }

    pub(crate) fn place(&mut self, index: u8, choice: u8) -> ChangeSet {
        let index_usize = usize::from(index);
        if self.state.settled || self.state.complete || choice >= 3 || !self.valid(index_usize) {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        let tile = self.state.offer[usize::from(choice)];
        self.state.board[index_usize] = tile;
        self.state.discovered |= 1 << tile as u8;
        self.state.turn += 1;
        self.message = TideMessage::Placed(tile);
        if self.state.turn % 6 == 0 {
            let season = usize::from(self.state.turn / 6 - 1);
            let forecast = self.state.forecasts[season];
            let gained: u16 = (0..BOARD_SIZE)
                .map(|cell| self.tile_score(cell, forecast))
                .sum();
            self.state.score = self.state.score.saturating_add(gained);
            self.state.harvests[season] = gained;
            self.state.harvest_len = self.state.harvest_len.max(season as u8 + 1);
            self.message = TideMessage::Harvest(season as u8 + 1, gained);
        }
        if self.state.turn == 24 {
            let goal_met = self.goal_progress() >= self.goal().count;
            if goal_met {
                self.state.score = self.state.score.saturating_add(45);
            }
            self.state.settled = true;
            let mut rng = Rng::new(self.state.rng);
            let mut available = [Perk::Forest; 8];
            let mut len = 0;
            for perk in Perk::ALL {
                if !self.has_perk(perk) {
                    available[len] = perk;
                    len += 1;
                }
            }
            for cursor in (1..len).rev() {
                let other = rng.index((cursor + 1) as u32);
                available.swap(cursor, other);
            }
            self.state.perk_offer.copy_from_slice(&available[..3]);
            self.state.rng = rng.0;
            self.message = TideMessage::Settled(goal_met);
        } else {
            self.refill();
        }
        self.pending = None;
        self.selected = Some(index);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn reroll(&mut self) -> ChangeSet {
        if self.state.settled || self.state.complete || self.state.rerolls == 0 {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        self.state.rerolls -= 1;
        self.refill();
        self.pending = None;
        self.selected = None;
        self.message = TideMessage::Rerolled;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.state.complete {
            return ChangeSet::NONE;
        }
        let Some(state) = self.history.pop() else {
            return ChangeSet::NONE;
        };
        self.state = state;
        self.pending = None;
        self.selected = None;
        self.modal = TideModal::None;
        self.message = TideMessage::Undone;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_modal(&mut self, modal: TideModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.pending = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn continue_voyage(&mut self, perk: Option<Perk>) -> ChangeSet {
        if !self.state.settled || self.state.complete {
            return ChangeSet::NONE;
        }
        if self.state.chapter < 3
            && !perk.is_some_and(|choice| self.state.perk_offer.contains(&choice))
        {
            return ChangeSet::NONE;
        }
        let index = usize::from(self.state.result_len);
        self.state.results[index] = IslandResult {
            score: self.state.score,
            target: self.state.target,
            goal: self.goal_progress() >= self.goal().count,
        };
        self.state.result_len += 1;
        self.state.total = self.state.total.saturating_add(self.state.score);
        if self.state.chapter == 3 {
            self.state.complete = true;
            self.modal = TideModal::Result;
            self.message = TideMessage::Complete;
        } else {
            self.state.perks |= perk.expect("validated perk").bit();
            self.state.chapter += 1;
            self.make_island();
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }
}

fn adjacent(index: usize) -> [Option<usize>; 4] {
    let x = index % 6;
    let y = index / 6;
    [
        (x > 0).then(|| index - 1),
        (x < 5).then(|| index + 1),
        (y > 0).then(|| index - 6),
        (y < 5).then(|| index + 6),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best_move(model: &TideModel) -> (u8, u8) {
        let mut best = None;
        for index in 0..BOARD_SIZE {
            for choice in 0..3 {
                if let Some((_, delta)) = model.preview(index, choice) {
                    let goal_bonus = i16::from(model.offer(choice) == model.goal().tile) * 2;
                    if best.is_none_or(|(_, value, _, _)| delta + goal_bonus > value) {
                        best = Some((index as u8, delta + goal_bonus, choice as u8, delta));
                    }
                }
            }
        }
        let (index, _, choice, _) = best.expect("legal placement");
        (index, choice)
    }

    fn solve_island(model: &mut TideModel) {
        while !model.settled() {
            let (index, choice) = best_move(model);
            assert!(model.place(index, choice).contains(ChangeSet::MODEL));
        }
    }

    #[test]
    fn reference_seed_matches_exported_initial_state() {
        let model = TideModel::default();
        assert_eq!(model.seed(), 4096);
        assert_eq!(model.terrain(0), 1);
        assert_eq!(model.terrain(1), 0);
        assert_eq!(model.tile(14), Tile::Beacon);
        assert_eq!(model.tile(15), Tile::Grove);
        assert_eq!(model.tile(20), Tile::Lagoon);
        assert_eq!(model.offer(0), Tile::Beacon);
        assert_eq!(model.offer(1), Tile::Lens);
        assert_eq!(model.offer(2), Tile::Harbor);
        assert_eq!(model.goal().name, "邻里计划");
    }

    #[test]
    fn preview_is_pure_and_adjacency_is_cardinal() {
        let model = TideModel::new(8);
        assert!(!model.valid(0));
        assert!(model.valid(13));
        assert!(!model.valid(14));
        let before = model;
        assert!(model.preview(13, 0).is_some());
        assert_eq!(model.state, before.state);
        assert_eq!(model.history.len, before.history.len);
    }

    #[test]
    fn scoring_preserves_water_flood_and_distinct_neighbour_rules() {
        let mut model = TideModel::new(4);
        model.state.board.fill(Tile::Sea);
        model.state.terrain.fill(1);
        model.state.board[14] = Tile::Field;
        assert_eq!(model.tile_score(14, Forecast::default()), 10);
        model.state.terrain[14] = 0;
        assert_eq!(
            model.tile_score(
                14,
                Forecast {
                    tide: TideLevel::High,
                    weather: Weather::Clear
                }
            ),
            0
        );
        model.state.board[15] = Tile::Dike;
        assert!(
            model.tile_score(
                14,
                Forecast {
                    tide: TideLevel::High,
                    weather: Weather::Clear
                }
            ) > 0
        );
        model.state.board.fill(Tile::Sea);
        model.state.board[14] = Tile::Hamlet;
        model.state.board[13] = Tile::Grove;
        model.state.board[15] = Tile::Grove;
        assert_eq!(model.tile_score(14, Forecast::default()), 5);
    }

    #[test]
    fn placement_harvest_and_undo_are_atomic() {
        let mut model = TideModel::new(9);
        let initial = model.state;
        let (index, choice) = best_move(&model);
        model.place(index, choice);
        assert_eq!(model.turn(), 1);
        model.undo();
        assert_eq!(model.state, initial);
        for _ in 0..6 {
            let (index, choice) = best_move(&model);
            model.place(index, choice);
        }
        assert_eq!(model.harvest_len(), 1);
        assert!(model.score() > 0);
    }

    #[test]
    fn full_campaign_completes_without_unbounded_state() {
        let mut model = TideModel::new(12);
        for island in 0..ISLAND_COUNT {
            solve_island(&mut model);
            assert_eq!(model.turn(), 24);
            assert_eq!(model.harvest_len(), 4);
            let perk = (island < 3).then(|| model.perk_offer(0));
            model.continue_voyage(perk);
        }
        assert!(model.complete());
        assert_eq!(model.state.result_len, 4);
        assert_eq!(
            model.total(),
            model
                .state
                .results
                .iter()
                .map(|result| result.score)
                .sum::<u16>()
        );
        assert!(core::mem::size_of::<TideModel>() <= 8 * 1024);
    }

    #[test]
    fn reroll_and_history_are_bounded() {
        let mut model = TideModel::new(15);
        for _ in 0..3 {
            assert!(model.reroll().contains(ChangeSet::MODEL));
        }
        assert!(!model.reroll().contains(ChangeSet::MODEL));
        while model.history_len() < HISTORY_CAPACITY as u8 {
            let (index, choice) = best_move(&model);
            model.place(index, choice);
        }
        assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
    }
}
