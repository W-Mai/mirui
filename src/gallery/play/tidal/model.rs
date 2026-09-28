use super::goals::GOALS;
use super::types::{
    BOARD_SIZE, Forecast, Goal, HISTORY_CAPACITY, ISLAND_COUNT, IslandResult, Perk, TideCommand,
    TideLevel, TideMessage, TideModal, Tile, Weather,
};
use crate::gallery::play::change::ChangeSet;

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
pub(super) struct TideState {
    pub(super) terrain: [u8; BOARD_SIZE],
    pub(super) board: [Tile; BOARD_SIZE],
    forecasts: [Forecast; 4],
    offer: [Tile; 3],
    perk_offer: [Perk; 3],
    harvests: [u16; 4],
    pub(super) results: [IslandResult; ISLAND_COUNT],
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
    pub(super) result_len: u8,
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
pub(super) struct TideHistory {
    entries: [TideState; HISTORY_CAPACITY],
    start: u8,
    pub(super) len: u8,
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
    pub(super) state: TideState,
    pub(super) history: TideHistory,
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

    pub(crate) fn apply_command(&mut self, command: TideCommand) -> ChangeSet {
        match command {
            TideCommand::Place { index, choice } => self.place(index, choice),
            TideCommand::Reroll => self.reroll(),
            TideCommand::Undo => self.undo(),
            TideCommand::Continue { perk } => self.continue_voyage(perk),
        }
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
