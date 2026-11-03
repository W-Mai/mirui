use crate::types::Fixed;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Default)]
pub(super) struct State {
    pub load: Load,
    pub expanded: bool,
    pub scroll: u32,
    viewport: Fixed,
    measures: Vec<Measure>,
    reading: Option<(Section, usize)>,
    last_scroll_input: u32,
}

#[derive(Default)]
struct Measure {
    height: Fixed,
    text: [Fixed; 2],
    facts: Vec<[Fixed; 2]>,
}

impl State {
    pub fn prepare_geometry(&mut self) {
        self.reading = None;
        self.last_scroll_input = 0;
        let sections = [Section::Core, Section::Firmware, Section::Saves];
        let count = sections
            .into_iter()
            .map(|section| self.rows(section).len())
            .max()
            .unwrap_or(0)
            .max(3);
        self.measures.resize_with(count, Measure::default);
        for index in 0..count {
            let fields = sections
                .into_iter()
                .filter_map(|section| self.rows(section).get(index))
                .map(|row| row.facts.len())
                .max()
                .unwrap_or(0);
            let measure = &mut self.measures[index];
            measure.height = Fixed::ZERO;
            measure.text = [Fixed::ZERO; 2];
            measure.facts.resize(fields, [Fixed::ZERO; 2]);
            measure.facts.fill([Fixed::ZERO; 2]);
        }
    }
    pub fn viewport(&mut self, height: Fixed) {
        self.viewport = height;
    }
    pub fn measure(&mut self, index: usize, height: Fixed) {
        if let Some(measure) = self.measures.get_mut(index) {
            measure.height = height;
        }
    }
    pub fn text_size(&mut self, index: usize, part: usize, height: Fixed) -> bool {
        let Some(size) = self
            .measures
            .get_mut(index)
            .and_then(|measure| measure.text.get_mut(part))
        else {
            return false;
        };
        let changed = *size != height;
        *size = height;
        changed
    }
    pub fn fact_measure(&mut self, index: usize, field: usize, value: bool, height: Fixed) -> bool {
        let Some(size) = self
            .measures
            .get_mut(index)
            .and_then(|measure| measure.facts.get_mut(field))
        else {
            return false;
        };
        let part = usize::from(value);
        let changed = size[part] != height;
        size[part] = height;
        changed
    }
    pub fn fact_height(&self, index: usize, field: usize) -> Fixed {
        self.measures
            .get(index)
            .and_then(|measure| measure.facts.get(field))
            .map_or(14.into(), |size| size[0].max(size[1]).max(14.into()))
    }
    pub fn facts_size(&self, section: Section, index: usize, gap: i32) -> Fixed {
        let count = self
            .rows(section)
            .get(index)
            .map_or(0, |row| row.facts.len());
        (0..count).fold(Fixed::ZERO, |sum, field| {
            sum + self.fact_height(index, field)
        }) + Fixed::from_int(count as i32 + gap * count.saturating_sub(1) as i32)
    }
    pub fn size(&self, section: Section, selected: usize, index: usize, gap: i32) -> Fixed {
        let sizes = self
            .measures
            .get(index)
            .map_or([Fixed::ZERO; 2], |measure| measure.text);
        sizes[0].max(26.into())
            + sizes[1].max(14.into())
            + if selected == index && self.expanded {
                self.facts_size(section, index, gap) + Fixed::from_int(9)
            } else {
                Fixed::ZERO
            }
            + Fixed::from_int(gap * 2 + 8)
    }
    pub fn offset(
        &mut self,
        section: Section,
        selected: usize,
        count: usize,
        previous: Fixed,
    ) -> Fixed {
        let top = self
            .measures
            .iter()
            .take(selected)
            .fold(Fixed::ZERO, |sum, measure| sum + measure.height);
        let height = self
            .measures
            .get(selected)
            .map_or(Fixed::ZERO, |measure| measure.height);
        let total = self
            .measures
            .iter()
            .take(count)
            .fold(Fixed::ZERO, |sum, measure| sum + measure.height);
        let maximum = (total - self.viewport).max(Fixed::ZERO);
        let offset = if self.expanded {
            let reading = Some((section, selected));
            let offset = if self.reading == reading {
                previous + Fixed::from_int(self.scroll as i32 - self.last_scroll_input as i32)
            } else {
                top + Fixed::from_int(self.scroll as i32)
            };
            self.reading = reading;
            self.last_scroll_input = self.scroll;
            offset.max(top.min(maximum))
        } else if top < previous {
            top
        } else if top + height > previous + self.viewport {
            top + height - self.viewport
        } else {
            previous
        };
        if !self.expanded {
            self.reading = None;
            self.last_scroll_input = 0;
        }
        offset.clamp(Fixed::ZERO, maximum)
    }
    pub fn rows(&self, section: Section) -> &'static [Row] {
        match &self.load {
            Load::Ready { document } => document.rows(section),
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Overview,
    Core,
    Firmware,
    Saves,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileStatus {
    Found,
    Missing,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    Required,
    Optional,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub value: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    pub summary: &'static str,
    pub facts: &'static [Fact],
    pub path: &'static str,
    pub status: FileStatus,
    pub requirement: Option<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub key: u32,
    pub title: String,
    pub platform: &'static str,
    pub core: &'static [Row],
    pub firmware: &'static [Row],
    pub saves: &'static [Row],
}

impl Document {
    pub fn example(key: u32, title: String, platform: &'static str) -> Self {
        Self {
            key,
            title,
            platform,
            core: CORE,
            firmware: FIRMWARE,
            saves: SAVES,
        }
    }
    pub fn rows(&self, section: Section) -> &'static [Row] {
        match section {
            Section::Overview => &[],
            Section::Core => self.core,
            Section::Firmware => self.firmware,
            Section::Saves => self.saves,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Load {
    #[default]
    Loading,
    Ready {
        document: Document,
    },
    Failed {
        message: &'static str,
    },
}

const CORE: &[Row] = &[
    Row {
        name: "选择规则",
        summary: "按共享顺序核验可用核心",
        path: "/roms/nes/魂斗罗.nes",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[
            Fact {
                label: "路径",
                value: "/roms/nes/魂斗罗.nes",
            },
            Fact {
                label: "前端",
                value: "retroarch",
            },
        ],
    },
    Row {
        name: "nestopia_nearby",
        summary: "工具优化",
        path: "/opt/arkos-nearby/cores/nestopia_nearby_libretro.so",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[
            Fact {
                label: "路径",
                value: "/opt/arkos-nearby/cores/nestopia_nearby_libretro.so",
            },
            Fact {
                label: "前端",
                value: "retroarch",
            },
            Fact {
                label: "核心",
                value: "Nestopia",
            },
            Fact {
                label: "版本",
                value: "1.53.27dfdc25-footer1",
            },
        ],
    },
    Row {
        name: "nestopia",
        summary: "设备原有",
        path: "/home/ark/.config/retroarch/cores/nestopia_libretro.so",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[
            Fact {
                label: "路径",
                value: "/home/ark/.config/retroarch/cores/nestopia_libretro.so",
            },
            Fact {
                label: "前端",
                value: "retroarch",
            },
        ],
    },
];

const FIRMWARE: &[Row] = &[Row {
    name: "disksys.rom",
    summary: "未找到文件",
    path: "/roms/bios/disksys.rom",
    status: FileStatus::Missing,
    requirement: Some(Requirement::Optional),
    facts: &[
        Fact {
            label: "路径",
            value: "/roms/bios/disksys.rom",
        },
        Fact {
            label: "要求",
            value: "可选",
        },
        Fact {
            label: "核心",
            value: "Nestopia",
        },
        Fact {
            label: "声明文件",
            value: "/home/ark/.config/retroarch/cores/nestopia_libretro.info",
        },
    ],
}];

const SAVES: &[Row] = &[
    Row {
        name: "单机配置",
        summary: "原前端配置",
        path: "/home/ark/.config/retroarch/retroarch.cfg",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[Fact {
            label: "路径",
            value: "/home/ark/.config/retroarch/retroarch.cfg",
        }],
    },
    Row {
        name: "SRAM 目录",
        summary: "配置目录",
        path: "~/.config/retroarch/saves",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[
            Fact {
                label: "目录",
                value: "~/.config/retroarch/saves",
            },
            Fact {
                label: "配置文件",
                value: "/home/ark/.config/retroarch/retroarch.cfg",
            },
        ],
    },
    Row {
        name: "即时存档目录",
        summary: "配置目录",
        path: "~/.config/retroarch/states",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[
            Fact {
                label: "目录",
                value: "~/.config/retroarch/states",
            },
            Fact {
                label: "配置文件",
                value: "/home/ark/.config/retroarch/retroarch.cfg",
            },
        ],
    },
    Row {
        name: "核心选项",
        summary: "原前端选项来源",
        path: "/home/ark/.config/retroarch/retroarch-core-options.cfg",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[Fact {
            label: "路径",
            value: "/home/ark/.config/retroarch/retroarch-core-options.cfg",
        }],
    },
    Row {
        name: "联机存档",
        summary: "创建会话时分配私有目录",
        path: "/run/arkos-nearby-rust",
        status: FileStatus::Unknown,
        requirement: None,
        facts: &[Fact {
            label: "路径",
            value: "/run/arkos-nearby-rust",
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_reading_reverses_immediately_after_clamped_repeated_input() {
        let mut state = State {
            load: Load::Ready {
                document: Document::example(0, "Example".into(), "NES"),
            },
            ..Default::default()
        };
        state.prepare_geometry();
        state.viewport(100.into());
        for index in 0..5 {
            state.measure(index, 80.into());
        }
        state.expanded = true;
        let mut offset = state.offset(Section::Saves, 0, 5, Fixed::ZERO);
        state.scroll = 8192;
        offset = state.offset(Section::Saves, 0, 5, offset);
        assert_eq!(offset, 300.into());
        state.scroll = 8168;
        offset = state.offset(Section::Saves, 0, 5, offset);
        assert_eq!(offset, 276.into());
        assert_eq!(state.offset(Section::Saves, 0, 5, offset), offset);
    }

    #[test]
    fn detail_reading_geometry_and_origin_changes_do_not_replay_consumed_input() {
        let mut state = State {
            load: Load::Ready {
                document: Document::example(0, "Example".into(), "NES"),
            },
            ..Default::default()
        };
        state.prepare_geometry();
        state.viewport(100.into());
        for index in 0..5 {
            state.measure(index, 80.into());
        }
        state.expanded = true;
        state.scroll = 8192;
        let mut offset = state.offset(Section::Saves, 0, 5, Fixed::ZERO);
        assert_eq!(offset, 300.into());
        state.viewport(200.into());
        offset = state.offset(Section::Saves, 0, 5, offset);
        assert_eq!(offset, 200.into());
        assert_eq!(state.offset(Section::Saves, 0, 5, offset), offset);
        state.scroll -= 24;
        offset = state.offset(Section::Saves, 0, 5, offset);
        assert_eq!(offset, 176.into());
        state.scroll = 0;
        offset = state.offset(Section::Core, 0, 3, offset);
        assert_eq!(offset, Fixed::ZERO);
        offset = state.offset(Section::Core, 1, 3, offset);
        assert_eq!(offset, 40.into());
        state.expanded = false;
        offset = state.offset(Section::Core, 0, 3, offset);
        assert_eq!(offset, Fixed::ZERO);
        assert_eq!(state.reading, None);
        state.expanded = true;
        assert_eq!(state.offset(Section::Core, 0, 3, offset), Fixed::ZERO);
        state.prepare_geometry();
        assert_eq!(state.reading, None);
        assert_eq!(state.last_scroll_input, 0);
    }

    #[test]
    fn detail_measurements_reuse_prepared_storage_and_bound_focused_scrolling() {
        let mut state = State {
            load: Load::Ready {
                document: Document::example(0, "Example".into(), "NES"),
            },
            ..Default::default()
        };
        state.prepare_geometry();
        let rows = state.measures.as_ptr();
        let facts = state
            .measures
            .iter()
            .map(|measure| measure.facts.as_ptr())
            .collect::<Vec<_>>();
        state.viewport(100.into());
        for index in 0..5 {
            state.measure(index, 80.into());
            state.text_size(index, 0, 26.into());
            state.text_size(index, 1, 14.into());
        }
        assert_eq!(state.offset(Section::Saves, 4, 5, Fixed::ZERO), 300.into());
        state.expanded = true;
        state.scroll = 8192;
        assert_eq!(state.offset(Section::Saves, 4, 5, Fixed::ZERO), 300.into());
        for _ in 0..20 {
            for (index, row) in SAVES.iter().enumerate() {
                for field in 0..row.facts.len() {
                    state.fact_measure(index, field, false, 14.into());
                    state.fact_measure(index, field, true, 42.into());
                }
            }
            state.prepare_geometry();
        }
        assert_eq!(state.measures.as_ptr(), rows);
        assert_eq!(
            state
                .measures
                .iter()
                .map(|measure| measure.facts.as_ptr())
                .collect::<Vec<_>>(),
            facts
        );
    }
}
