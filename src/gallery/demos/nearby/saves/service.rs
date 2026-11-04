use super::schema::*;
use alloc::{format, rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;

pub(crate) type Control = Rc<RefCell<Service>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Success,
    ReadFailure,
    WriteFailure,
    Conflict,
    Empty,
    ActiveRoom,
}
impl Scenario {
    pub const ALL: [Self; 6] = [
        Self::Success,
        Self::ReadFailure,
        Self::WriteFailure,
        Self::Conflict,
        Self::Empty,
        Self::ActiveRoom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "Save succeeds",
            Self::ReadFailure => "Read fails",
            Self::WriteFailure => "Write fails",
            Self::Conflict => "Record changed",
            Self::Empty => "No save files",
            Self::ActiveRoom => "Room in use",
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
    close_after: bool,
    scenario: Scenario,
}
impl Service {
    pub fn view(&self) -> Option<&View> {
        self.view.as_ref()
    }
    pub fn scenario(&self) -> Scenario {
        self.scenario
    }
    pub fn set_scenario(&mut self, value: Scenario) {
        self.scenario = value;
        let active = value == Scenario::ActiveRoom;
        if let Some((_, document)) = self.documents.iter_mut().find(|(key, _)| *key == self.key) {
            document.room_active = active;
        }
        if let Some(document) = self.view.as_mut().and_then(|view| view.document.as_mut()) {
            document.room_active = active;
        }
    }

    pub fn open(&mut self, key: u32, title: String, room_active: bool) {
        if !self.documents.iter().any(|(id, _)| *id == key) {
            if self.documents.len() == 8 {
                return;
            }
            if self.documents.is_empty() {
                self.documents.reserve_exact(8);
            }
            let mut document = super::fixture().document.unwrap();
            document.title = title;
            document.records.reserve(48 - document.records.len());
            for (index, record) in document.records.iter_mut().enumerate() {
                if index % 3 == 0 && record.kind == Kind::State {
                    record.thumbnail = "nearby_save_preview".into();
                    record.thumbnail_size = (48, 32);
                }
            }
            self.documents.push((key, document));
        }
        self.key = key;
        self.epoch = self.epoch.wrapping_add(1);
        self.close_after = false;
        self.documents
            .iter_mut()
            .find(|(id, _)| *id == key)
            .unwrap()
            .1
            .room_active = room_active || self.scenario == Scenario::ActiveRoom;
        self.view = Some(View {
            epoch: self.epoch,
            busy: true,
            ..Default::default()
        });
        self.pending = Some((180, Request::Reload { epoch: self.epoch }));
    }
    pub fn reference(&mut self) {
        self.close();
        let view = super::fixture();
        self.key = 0;
        self.documents.clear();
        self.documents
            .push((0, view.document.as_ref().unwrap().clone()));
        self.epoch = view.epoch;
        self.view = Some(view);
    }
    pub fn close(&mut self) {
        self.view = None;
        self.pending = None;
        self.close_after = false;
    }
    pub fn submit(&mut self, request: &Request) -> bool {
        let Some(view) = self.view.as_mut() else {
            return false;
        };
        if request.epoch() != view.epoch || !request.valid() {
            return false;
        }
        if matches!(request, Request::Close { .. }) {
            if self.pending.is_some() {
                self.close_after = true;
            } else {
                self.close();
            }
            return true;
        }
        if self.pending.is_some() {
            return false;
        }
        view.busy = true;
        view.error.clear();
        view.notice.clear();
        self.pending = Some((220, request.clone()));
        true
    }
    pub fn tick(&mut self, delta: u16) -> bool {
        let Some((remaining, _)) = self.pending.as_mut() else {
            return false;
        };
        *remaining = remaining.saturating_sub(delta);
        if *remaining != 0 {
            return false;
        }
        let (_, request) = self.pending.take().unwrap();
        let Some(view) = self.view.as_mut() else {
            return false;
        };
        if view.epoch != request.epoch() {
            return false;
        }
        view.busy = false;
        view.completed = view.completed.wrapping_add(1);
        let Some((_, document)) = self.documents.iter_mut().find(|(id, _)| *id == self.key) else {
            return false;
        };
        match request {
            Request::Reload { .. } if self.scenario == Scenario::ReadFailure => {
                view.error = "模拟读取失败，请重新读取".into();
            }
            Request::Reload { .. } => {
                let mut current = document.clone();
                if self.scenario == Scenario::Empty {
                    current.records.clear();
                }
                view.document = Some(current);
                view.error.clear();
            }
            Request::Manage {
                key,
                revision,
                operation,
                tab,
                ..
            } => {
                if document.room_active {
                    view.error = "退出房间后可管理单机存档".into();
                } else if self.scenario == Scenario::WriteFailure {
                    view.error = "模拟操作失败，原存档已保留".into();
                } else if self.scenario == Scenario::Conflict {
                    view.error = "存档已变更，请重新读取".into();
                } else if let Some(index) = document.records.iter().position(|r| r.key == key) {
                    if document.records[index].revision != revision
                        || !document.records[index].can(operation)
                    {
                        view.error = "存档已变更或操作不可用".into();
                    } else {
                        let result = operate(document, index, operation, &mut self.revision);
                        match result {
                            Ok(()) => {
                                view.document = Some(document.clone());
                                view.notice = match operation {
                                    Operation::Backup => "已创建模拟备份",
                                    Operation::Delete => "已备份并删除模拟存档",
                                    Operation::Restore => "已恢复模拟备份",
                                    Operation::Load => "模拟载入成功",
                                    Operation::Start => "模拟启动成功",
                                }
                                .into();
                                view.return_key = key;
                                view.return_tab = tab;
                            }
                            Err(message) => view.error = message.into(),
                        }
                    }
                } else {
                    view.error = "存档已不存在".into();
                }
            }
            Request::Start { .. } => {
                if document.room_active {
                    view.error = "退出房间后可开始单机游戏".into();
                } else {
                    view.notice = "模拟启动成功".into();
                }
            }
            Request::Close { .. } => {}
        }
        if self.close_after {
            self.close();
        }
        true
    }
}

fn backup(document: &mut Document, index: usize, revision: &mut u64) -> Result<(), &'static str> {
    if document.records.len() >= 48 {
        return Err("模拟存档容量已满，原记录已保留");
    }
    *revision = revision.wrapping_add(1);
    let mut record = document.records[index].clone();
    record.kind = Kind::Backup;
    record.key = format!("backup-{revision}");
    record.path = format!("/demo/backups/{}.bak", record.key);
    record.title = format!("{} 备份", record.title);
    record.revision = format!("{revision:064x}");
    document.records.push(record);
    Ok(())
}
fn operate(
    document: &mut Document,
    index: usize,
    operation: Operation,
    revision: &mut u64,
) -> Result<(), &'static str> {
    match operation {
        Operation::Backup => backup(document, index, revision)?,
        Operation::Delete => {
            if document.records[index].kind != Kind::Backup {
                backup(document, index, revision)?;
            }
            document.records.remove(index);
        }
        Operation::Restore => {
            let source = document.records[index].clone();
            let kind = if source.slot.is_none() {
                Kind::Sram
            } else {
                Kind::State
            };
            if let Some(target) = document
                .records
                .iter()
                .position(|r| r.kind == kind && r.slot == source.slot)
            {
                backup(document, target, revision)?;
                let target_key = document.records[target].key.clone();
                let target_path = document.records[target].path.clone();
                document.records[target] = Record {
                    key: target_key,
                    path: target_path,
                    kind,
                    ..source
                };
            } else {
                if document.records.len() >= 48 {
                    return Err("模拟存档容量已满，原记录已保留");
                }
                document.records.push(Record {
                    key: format!("state-restored-{revision}"),
                    kind,
                    ..source
                });
            }
        }
        Operation::Load => {
            if let Some(sram) = document.records.iter().position(|r| r.kind == Kind::Sram) {
                backup(document, sram, revision)?;
            }
        }
        Operation::Start => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service() -> Service {
        let mut service = Service::default();
        service.open(1, "Demo".into(), false);
        service.tick(180);
        service
    }
    fn request(service: &Service, operation: Operation) -> Request {
        let view = service.view().unwrap();
        let record = &view.document.as_ref().unwrap().records[0];
        Request::Manage {
            epoch: view.epoch,
            key: record.key.clone(),
            revision: record.revision.clone(),
            operation,
            tab: Tab::States,
        }
    }
    #[test]
    fn busy_close_finishes_the_operation_before_retiring_its_session() {
        let mut s = service();
        assert!(s.submit(&request(&s, Operation::Backup)));
        let epoch = s.view().unwrap().epoch;
        assert!(s.submit(&Request::Close { epoch }));
        assert!(s.view().unwrap().busy);
        assert!(!s.tick(100));
        assert!(s.tick(120));
        assert!(s.view().is_none());
        assert_eq!(s.documents[0].1.records.len(), 14);
        s.open(1, "Demo".into(), false);
        assert!(!s.submit(&Request::Reload { epoch }));
    }
    #[test]
    fn failed_operations_leave_inventory_unchanged_and_reload_recovers() {
        let mut s = service();
        for scenario in [Scenario::WriteFailure, Scenario::Conflict] {
            s.set_scenario(scenario);
            let before = s.documents[0].1.clone();
            s.submit(&request(&s, Operation::Delete));
            s.tick(220);
            assert!(!s.view().unwrap().error.is_empty());
            assert_eq!(before, s.documents[0].1);
        }
        s.set_scenario(Scenario::Success);
        let epoch = s.view().unwrap().epoch;
        s.submit(&Request::Reload { epoch });
        s.tick(220);
        assert!(s.view().unwrap().error.is_empty());
    }
    #[test]
    fn restore_preserves_destination_identity_and_prepares_a_backup() {
        let mut s = service();
        let document = &mut s.documents[0].1;
        backup(document, 0, &mut s.revision).unwrap();
        let key = document.records[0].key.clone();
        let backup_index = document.records.len() - 1;
        operate(document, backup_index, Operation::Restore, &mut s.revision).unwrap();
        assert_eq!(document.records[0].key, key);
        assert_eq!(document.records.len(), 15);
    }

    #[test]
    fn actual_model_effects_preserve_hidden_settings_and_return_to_the_same_scope() {
        use crate::gallery::demos::nearby::{
            configuration,
            launcher::{Launcher, Overlay},
            wifi::Action,
        };
        use crate::prelude::*;
        let mut app = App::headless(8, 8);
        let launcher = app.add_model(Launcher::default()).finish();
        let output = launcher.configuration_control();
        launcher
            .configuration()
            .on_effect(move |request: &configuration::schema::Request| {
                output.borrow_mut().submit(request);
            })
            .unwrap();
        let output = launcher.saves_control();
        launcher
            .saves()
            .on_effect(move |request: &Request| {
                output.borrow_mut().submit(request);
            })
            .unwrap();
        launcher.choose_game(0, 0);
        launcher.open_configuration();
        launcher.animate(180);
        launcher.configuration_input(Action::Auxiliary);
        for _ in 0..3 {
            launcher.configuration_input(Action::Down);
        }
        launcher.configuration_input(Action::Confirm);
        let settings = launcher.configuration();
        let scope = settings.scope();
        settings.select(settings.count() - 2);
        let index = settings.selected();
        launcher.configuration_input(Action::Confirm);
        launcher.animate(0);
        assert_eq!(launcher.overlay(), Some(Overlay::Saves));
        assert!(!settings.active());
        launcher.animate(180);
        launcher.saves_input(Action::Back);
        launcher.animate(0);
        assert_eq!(launcher.overlay(), Some(Overlay::Configuration));
        assert!(settings.active());
        assert_eq!(settings.scope(), scope);
        assert_eq!(settings.section(), configuration::schema::Section::Saves);
        assert_eq!(settings.selected(), index);
        launcher.configuration_input(Action::Back);
        assert_eq!(settings.selected(), 3);
    }
}
