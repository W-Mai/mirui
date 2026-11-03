use super::schema::*;
use alloc::{format, rc::Rc, string::String, vec, vec::Vec};
use core::cell::RefCell;

pub(crate) type Control = Rc<RefCell<Service>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Success,
    Conflict,
    WriteFailure,
    ReadFailure,
}

impl Scenario {
    pub const ALL: [Self; 4] = [
        Self::Success,
        Self::Conflict,
        Self::WriteFailure,
        Self::ReadFailure,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "Save succeeds",
            Self::Conflict => "External change",
            Self::WriteFailure => "Write fails",
            Self::ReadFailure => "Read fails",
        }
    }
}

#[derive(Default)]
pub(crate) struct Service {
    documents: Vec<(u32, Document)>,
    key: u32,
    epoch: u64,
    revision: u64,
    view: Option<View>,
    pending: Option<(u16, Request)>,
    scenario: Scenario,
}

impl Service {
    pub fn view(&self) -> Option<&View> {
        self.view.as_ref()
    }

    pub fn scenario(&self) -> Scenario {
        self.scenario
    }

    pub fn set_scenario(&mut self, scenario: Scenario) {
        self.scenario = scenario;
    }

    pub fn open(&mut self, key: u32, title: String, platform: &str) {
        if !self.documents.iter().any(|(id, _)| *id == key) {
            if self.documents.is_empty() {
                self.documents.reserve_exact(8);
            }
            if self.documents.len() == 8 {
                crate::warn!("Nearby simulated configuration inventory is full");
                return;
            }
            self.documents.push((key, example(key, title, platform)));
        }
        self.key = key;
        self.epoch = self.epoch.wrapping_add(1);
        self.view = Some(View {
            epoch: self.epoch,
            ..View::default()
        });
        self.pending = Some((180, Request::Reload { epoch: self.epoch }));
    }

    pub fn reference(&mut self) {
        self.view = Some(super::fixture());
        self.epoch = self.view.as_ref().unwrap().epoch;
        self.pending = None;
    }

    pub fn close(&mut self) {
        self.pending = None;
        self.view = None;
    }

    pub fn submit(&mut self, request: &Request) -> bool {
        let Some(view) = &mut self.view else {
            return false;
        };
        if request.epoch() != view.epoch {
            return false;
        }
        if matches!(request, Request::Close { .. }) {
            self.close();
            return true;
        }
        if view.saving {
            return false;
        }
        view.error.clear();
        if matches!(request, Request::Save { .. }) {
            view.saving = true;
        }
        self.pending = Some((180, request.clone()));
        true
    }

    pub fn tick(&mut self, delta: u16) -> bool {
        let Some((remaining, _)) = &mut self.pending else {
            return false;
        };
        *remaining = remaining.saturating_sub(delta);
        if *remaining != 0 {
            return false;
        }
        let (_, request) = self.pending.take().unwrap();
        let Some(view) = &mut self.view else {
            return false;
        };
        if request.epoch() != view.epoch {
            return false;
        }
        let Some((_, document)) = self.documents.iter_mut().find(|(key, _)| *key == self.key)
        else {
            return false;
        };
        match request {
            Request::Reload { .. } => {
                view.error.clear();
                view.saving = false;
                view.load = if self.scenario == Scenario::ReadFailure {
                    Load::Failed {
                        message: "模拟读取失败，按 X 重试".into(),
                    }
                } else {
                    Load::Ready {
                        document: document.clone(),
                    }
                };
                view.projection = None;
            }
            Request::Preview {
                revision,
                scope,
                changes,
                ..
            }
            | Request::Save {
                revision,
                scope,
                changes,
                ..
            } => {
                let saving = view.saving;
                view.saving = false;
                if saving && self.scenario == Scenario::Conflict {
                    self.revision = self.revision.wrapping_add(1);
                    document.revision = format!("{:064x}", self.revision);
                }
                if revision != document.revision {
                    view.error = "配置文件已变更，请重新读取".into();
                } else if saving && self.scenario == Scenario::WriteFailure {
                    view.error = "模拟保存失败，草稿已保留".into();
                } else if changes.iter().any(|change| {
                    document
                        .fields
                        .iter()
                        .find(|field| field.id == change.id)
                        .is_none_or(|field| {
                            field.targets[scope.index()].is_empty()
                                || change
                                    .value
                                    .as_ref()
                                    .is_some_and(|value| !field.accepts(value))
                        })
                }) {
                    view.error = "设置项或范围无效".into();
                } else if saving {
                    for change in changes {
                        let field = document
                            .fields
                            .iter_mut()
                            .find(|field| field.id == change.id)
                            .unwrap();
                        let index = scope.index();
                        field.values[index] = change
                            .value
                            .unwrap_or_else(|| field.inherited[index].clone());
                        field.origins[index] = if field.values[index] == field.inherited[index] {
                            "继承值".into()
                        } else {
                            field.targets[index].clone()
                        };
                    }
                    self.revision = self.revision.wrapping_add(1);
                    document.revision = format!("{:064x}", self.revision);
                    view.saved = view.saved.wrapping_add(1);
                    document.saves = paths(document, Scope::Game, &[]);
                    view.load = Load::Ready {
                        document: document.clone(),
                    };
                    view.projection = None;
                } else {
                    view.projection = Some(Projection {
                        scope,
                        facts: paths(document, scope, &changes),
                        changes,
                    });
                }
            }
            Request::Close { .. } => unreachable!(),
        }
        true
    }
}

fn example(key: u32, title: String, platform: &str) -> Document {
    let Load::Ready { mut document } = super::fixture().load else {
        unreachable!()
    };
    document.key = format!("{key:064x}");
    document.title = title;
    document.platform = platform.into();
    document.core = match platform {
        "nes" => "FCEUmm",
        "snes" => "Snes9x",
        "gb" => "Gambatte",
        "megadrive" => "Genesis Plus GX",
        "psx" => "PCSX ReARMed",
        _ => "FBNeo",
    }
    .into();
    document.revision = format!("{:064x}", 0);
    document
        .fields
        .retain(|field| !matches!(field.id, Id::Option(_)));
    for field in &mut document.fields {
        field.targets = targets(&document.core, &document.title, "cfg");
        field.origins = core::array::from_fn(|_| "前端默认值".into());
    }
    let boolean = document.fields[0].choices.clone();
    let mut add = |id, section, label: &str, choices: Vec<Choice>, extension| {
        let initial = choices[0].value.clone();
        document.fields.push(Field {
            id,
            section,
            label: label.into(),
            values: core::array::from_fn(|_| initial.clone()),
            inherited: core::array::from_fn(|_| initial.clone()),
            choices,
            origins: core::array::from_fn(|_| "前端默认值".into()),
            targets: targets(&document.core, &document.title, extension),
        });
    };
    let texts = |values: &[&str]| {
        values
            .iter()
            .map(|text| Choice {
                value: Value::Text((*text).into()),
                label: (*text).into(),
            })
            .collect()
    };
    add(
        Id::Aspect,
        Section::Video,
        "画面比例",
        texts(&["核心默认", "4:3", "16:9", "全屏"]),
        "cfg",
    );
    add(
        Id::Option("region".into()),
        Section::Core,
        "区域",
        texts(&["自动", "日本", "美国", "欧洲"]),
        "opt",
    );
    add(
        Id::Option("sprites".into()),
        Section::Core,
        "移除精灵数量限制",
        boolean.clone(),
        "opt",
    );
    for (button, label) in ["A 按键", "B 按键", "X 按键", "Y 按键"]
        .into_iter()
        .enumerate()
    {
        add(
            Id::Button(button as u8),
            Section::Input,
            label,
            texts(&["A", "B", "X", "Y", "L", "R"]),
            "rmp",
        );
    }
    add(
        Id::Slot,
        Section::Saves,
        "即时存档槽位",
        (0..4)
            .map(|value| Choice {
                value: Value::Number(value),
                label: format!("槽位 {value}"),
            })
            .collect(),
        "cfg",
    );
    for (id, label) in [
        (Id::SaveByCore, "按核心保存 SRAM"),
        (Id::StateByCore, "按核心保存即时存档"),
        (Id::SaveInContent, "在游戏目录保存 SRAM"),
        (Id::StateInContent, "在游戏目录保存即时存档"),
    ] {
        add(id, Section::Saves, label, boolean.clone(), "cfg");
    }
    document.saves = paths(&document, Scope::Game, &[]);
    document.sources = vec![Fact::new(
        "说明",
        "本 Demo 仅模拟设置和写入，不读取或修改系统文件",
    )];
    document
}

fn paths(document: &Document, scope: Scope, changes: &[Change]) -> Vec<Fact> {
    let enabled = |id: Id| {
        document
            .fields
            .iter()
            .find(|field| field.id == id)
            .is_some_and(|field| {
                let value = changes.iter().find(|change| change.id == id).map_or(
                    &field.values[scope.index()],
                    |change| {
                        change
                            .value
                            .as_ref()
                            .unwrap_or(&field.inherited[scope.index()])
                    },
                );
                *value == Value::Bool(true)
            })
    };
    [
        ("SRAM 文件", "srm", Id::SaveByCore, Id::SaveInContent),
        ("即时存档", "state", Id::StateByCore, Id::StateInContent),
    ]
    .into_iter()
    .map(|(label, extension, by_core, in_content)| {
        let base = if enabled(in_content) {
            format!("/roms/{}", document.platform)
        } else {
            "/saves".into()
        };
        let directory = if enabled(by_core) {
            format!("{base}/{}", document.core)
        } else {
            base
        };
        Fact::new(label, format!("{directory}/{}.{extension}", document.title))
    })
    .collect()
}

fn targets(core: &str, title: &str, extension: &str) -> [String; 3] {
    [
        format!("/home/ark/.config/retroarch/config/{core}/{title}.{extension}"),
        format!("/home/ark/.config/retroarch/config/{core}/{core}.{extension}"),
        format!("/home/ark/.config/retroarch/retroarch.{extension}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded() -> Service {
        let mut service = Service::default();
        service.open(0, "魂斗罗".into(), "nes");
        assert!(service.tick(180));
        service
    }
    fn save(service: &Service, scope: Scope) -> Request {
        let view = service.view().unwrap();
        let Load::Ready { document } = &view.load else {
            panic!()
        };
        Request::Save {
            epoch: view.epoch,
            revision: document.revision.clone(),
            scope,
            changes: vec![Change {
                id: Id::IntegerScale,
                value: Some(Value::Bool(true)),
            }],
        }
    }
    #[test]
    fn simulated_saves_are_scoped_persistent_and_epoch_checked() {
        let mut service = loaded();
        let request = save(&service, Scope::Game);
        assert!(service.submit(&request));
        assert!(!service.submit(&request));
        assert!(!service.tick(179));
        assert!(service.tick(1));
        let Load::Ready { document } = &service.view().unwrap().load else {
            panic!()
        };
        assert_eq!(
            document.fields[0].values,
            [Value::Bool(true), Value::Bool(false), Value::Bool(false)]
        );
        service.close();
        service.open(0, "魂斗罗".into(), "nes");
        assert!(!service.submit(&request));
        service.tick(180);
        let Load::Ready { document } = &service.view().unwrap().load else {
            panic!()
        };
        assert_eq!(document.fields[0].values[0], Value::Bool(true));
        service.open(1, "other".into(), "gb");
        service.tick(180);
        let Load::Ready { document } = &service.view().unwrap().load else {
            panic!()
        };
        assert_eq!(document.fields[0].values[0], Value::Bool(false));
    }
    #[test]
    fn errors_do_not_acknowledge_saves_and_reloading_uses_current_revision() {
        let mut service = loaded();
        for scenario in [Scenario::WriteFailure, Scenario::Conflict] {
            service.set_scenario(scenario);
            let request = save(&service, Scope::Core);
            service.submit(&request);
            service.tick(180);
            assert_eq!(service.view().unwrap().saved, 0);
            assert!(!service.view().unwrap().error.is_empty());
            assert!(!service.view().unwrap().saving);
            service.submit(&Request::Reload {
                epoch: service.epoch,
            });
            service.tick(180);
            assert!(service.view().unwrap().error.is_empty());
        }
        service.set_scenario(Scenario::Success);
        assert!(service.submit(&save(&service, Scope::Core)));
        service.tick(180);
        assert_eq!(service.view().unwrap().saved, 1);
    }

    #[test]
    fn model_effects_complete_scoped_saves_and_retire_closed_requests() {
        use crate::gallery::demos::nearby::{launcher::Launcher, wifi::Action};
        use crate::prelude::*;
        let mut app = App::headless(8, 8);
        let launcher = app.add_model(Launcher::default()).finish();
        let output = launcher.configuration_control();
        launcher
            .configuration()
            .on_effect(move |request: &Request| {
                output.borrow_mut().submit(request);
            })
            .unwrap();
        launcher.choose_game(0, 0);
        launcher.open_configuration();
        assert!(launcher.configuration().loading());
        launcher.animate(180);
        assert!(!launcher.configuration().loading());
        launcher.configuration_input(Action::Confirm);
        launcher.configuration_input(Action::Right);
        assert!(launcher.configuration().dirty());
        launcher.configuration_input(Action::Auxiliary);
        assert_eq!(launcher.configuration().section(), Section::Confirm);
        launcher.configuration_input(Action::Confirm);
        assert!(launcher.configuration().saving());
        launcher.animate(180);
        assert_eq!(launcher.configuration().scope(), Scope::Core);
        assert_eq!(launcher.configuration().section(), Section::Video);
        assert!(!launcher.configuration().dirty());
        launcher.configuration_input(Action::Back);
        launcher.configuration_input(Action::Back);
        launcher.animate(0);
        assert!(!launcher.configuration().active());
        assert_eq!(launcher.menu_selection(), 4);
        launcher
            .configuration_control()
            .borrow_mut()
            .submit(&Request::Close { epoch: 1 });
        launcher.animate(0);
        assert_eq!(launcher.menu_selection(), 4);
    }
}
