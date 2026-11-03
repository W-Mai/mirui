use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Game,
    Core,
    Frontend,
}
impl Scope {
    pub const ALL: [Self; 3] = [Self::Game, Self::Core, Self::Frontend];
    pub fn index(self) -> usize {
        match self {
            Self::Game => 0,
            Self::Core => 1,
            Self::Frontend => 2,
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % 3]
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Game => "当前游戏",
            Self::Core => "当前核心",
            Self::Frontend => "前端",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Home,
    Video,
    Core,
    Input,
    Saves,
    Sources,
    Confirm,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Id {
    IntegerScale,
    Aspect,
    AutoLoad,
    Slot,
    SaveByCore,
    StateByCore,
    SaveInContent,
    StateInContent,
    Option(String),
    Button(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Number(i32),
    Text(String),
}
impl Value {
    pub fn label(&self) -> String {
        match self {
            Self::Bool(v) => if *v { "开启" } else { "关闭" }.into(),
            Self::Number(v) => v.to_string(),
            Self::Text(v) => v.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: Value,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub id: Id,
    pub section: Section,
    pub label: String,
    pub choices: Vec<Choice>,
    pub values: [Value; 3],
    pub origins: [String; 3],
    pub targets: [String; 3],
    pub inherited: [Value; 3],
}
impl Field {
    pub fn accepts(&self, value: &Value) -> bool {
        self.choices.iter().any(|choice| &choice.value == value)
    }
    pub fn label_for(&self, value: &Value) -> String {
        self.choices
            .iter()
            .find(|c| &c.value == value)
            .map(|c| c.label.clone())
            .unwrap_or_else(|| value.label())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub key: String,
    pub title: String,
    pub platform: String,
    pub core: String,
    pub revision: String,
    pub fields: Vec<Field>,
    pub saves: Vec<Fact>,
    pub sources: Vec<Fact>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Load {
    #[default]
    Loading,
    Ready {
        document: Document,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub epoch: u64,
    pub load: Load,
    pub saving: bool,
    pub saved: u64,
    pub error: String,
    pub projected: Vec<Fact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub id: Id,
    pub value: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Save {
        epoch: u64,
        revision: String,
        scope: Scope,
        changes: Vec<Change>,
    },
    Preview {
        epoch: u64,
        revision: String,
        scope: Scope,
        changes: Vec<Change>,
    },
    Reload {
        epoch: u64,
    },
    Close {
        epoch: u64,
    },
}
impl Request {
    pub fn epoch(&self) -> u64 {
        match self {
            Self::Save { epoch, .. }
            | Self::Preview { epoch, .. }
            | Self::Reload { epoch }
            | Self::Close { epoch } => *epoch,
        }
    }
}
