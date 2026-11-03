use alloc::string::String;

#[derive(Default)]
pub(super) struct State {
    pub load: Load,
    pub expanded: bool,
    pub scroll: u32,
}

impl State {
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
