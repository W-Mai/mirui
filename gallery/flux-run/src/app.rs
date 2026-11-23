use crate::{
    gameplay::{Controls, Cue, Game, Mode, Session},
    presentation::{self, Marks, SceneTone, build},
    world::District,
};
use mirui::{
    core::model::ModelHandle,
    ecs::DeltaTimeMs,
    input::event::{
        input::{Key, PhysicalKey},
        lifecycle::{HostFocusChanged, HostVisibilityChanged},
    },
    prelude::*,
};
use mirui_sprite_stack::{SpriteStackPlugin, StackStage};

#[derive(Clone, Copy)]
pub(crate) enum Control {
    Up,
    Right,
    Down,
    Left,
    Magnet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Hud {
    pub session: Session,
    pub mode: Mode,
    pub level: u32,
    pub name: &'static str,
    pub cores: u8,
    pub quota: u8,
    pub life: u8,
    pub seconds: u32,
    pub revivals: u32,
    pub score: u32,
    pub best: u32,
    pub tether: bool,
    pub target: bool,
}

impl Hud {
    fn from_game(game: &Game) -> Self {
        Self {
            session: game.session,
            mode: game.mode,
            level: game.level,
            name: match game.harbor.district {
                District::Outer => "OUTER HARBOR",
                District::Locks => "TWIN LOCKS",
                District::Storm => "STORM DOCK",
            },
            cores: game.result.collected,
            quota: game.harbor.quota,
            life: game.life,
            seconds: game.result.ticks / 63,
            revivals: game.result.revivals,
            score: if matches!(game.session, Session::Departed | Session::Results) {
                game.total
            } else {
                game.total.saturating_add(game.result.score)
            },
            best: game.best_for_run(),
            tether: game.tether.is_some(),
            target: game.target().is_some(),
        }
    }
}

#[mirui::model]
pub struct FluxRun {
    pub(crate) stage: model!(owned StackStage),
    pub(crate) owner: Entity,
    pub(crate) game: Game,
    pub(crate) compact: bool,
    #[observe]
    pub(crate) tone: SceneTone,
    #[observe]
    pub(crate) marks: Marks,
    #[observe]
    pub(crate) hud: Hud,
    #[observe]
    pub(crate) camera_scale: Fixed,
    #[observe]
    pub(crate) bridge_x: i32,
    held: u8,
    touch: u8,
    suspended: bool,
    focused: bool,
}

impl FluxRun {
    pub fn level(&self) -> u32 {
        self.game.level
    }

    pub fn recovered(&self) -> u8 {
        self.game.result.collected
    }

    fn new(owner: Entity, compact: bool, tone: SceneTone) -> Self {
        let game = Game::new(0x4652_554e);
        Self {
            stage: StackStage::bounded(presentation::STAGE_CAPACITY).into(),
            owner,
            marks: Marks::from_game(&game),
            hud: Hud::from_game(&game),
            camera_scale: Fixed::from_int(4),
            bridge_x: game.harbor.bridge,
            game,
            compact,
            tone,
            held: 0,
            touch: 0,
            suspended: false,
            focused: true,
        }
    }

    fn refresh(&mut self) {
        self.bridge_x = self.game.harbor.bridge;
        self.marks = Marks::from_game(&self.game);
        self.hud = Hud::from_game(&self.game);
    }

    fn clear_controls(&mut self) {
        self.held = 0;
        self.touch = 0;
        self.game.cancel();
    }
}

#[mirui::model]
impl FluxRun {
    pub(crate) fn set_tone(&mut self, tone: SceneTone) {
        self.tone = tone;
    }

    pub(crate) fn fit_camera(&mut self, size: [u16; 2]) {
        let [width, height] = size;
        // The diagonal harbor and its perspective margin fit inside this footprint.
        self.camera_scale = Fixed::from_ratio(i32::from(width.saturating_sub(32).max(1)), 114)
            .min(Fixed::from_ratio(
                i32::from(height.saturating_sub(32).max(1)),
                91,
            ))
            .min(Fixed::from_int(6));
    }

    pub fn set_autoplay(&mut self, enabled: bool) {
        if enabled && self.game.session == Session::Title {
            self.start(Mode::Auto);
        } else {
            self.clear_controls();
            self.game
                .set_mode(if enabled { Mode::Auto } else { Mode::Manual });
            self.refresh();
        }
    }

    pub fn start(&mut self, mode: Mode) {
        self.clear_controls();
        self.game.set_mode(mode);
        self.game.start();
        self.refresh();
    }

    pub fn advance(&mut self, delta: u16) {
        if self.suspended || !self.focused {
            self.game.cue = Cue::None;
            return;
        }
        let buttons = self.held | self.touch;
        let pressed = |control: Control| i8::from(buttons & (1u8 << control as u8) != 0);
        self.game.elapsed_ms(
            delta,
            Controls {
                axis: presentation::steer_on_screen(
                    pressed(Control::Right) - pressed(Control::Left),
                    pressed(Control::Up) - pressed(Control::Down),
                ),
                magnet: pressed(Control::Magnet) != 0,
            },
        );
        self.refresh();
    }

    pub(crate) fn menu(&mut self) {
        self.clear_controls();
        self.game.pause();
        self.refresh();
    }

    pub(crate) fn next(&mut self) {
        self.clear_controls();
        self.game.next();
        self.refresh();
    }

    pub(crate) fn retry(&mut self) {
        self.clear_controls();
        self.game.restart();
        self.refresh();
    }

    pub(crate) fn title(&mut self) {
        self.clear_controls();
        self.game.session = Session::Title;
        self.refresh();
    }

    pub(crate) fn finish(&mut self) {
        self.clear_controls();
        self.game.finish();
        self.refresh();
    }

    pub(crate) fn button(&mut self, control: Control, down: bool) {
        set_button(&mut self.touch, control, down);
    }

    pub(crate) fn cancel_touch(&mut self, control: Control) {
        set_button(&mut self.touch, control, false);
        if matches!(control, Control::Magnet) {
            self.game.cancel();
        }
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.clear_controls();
        }
    }

    fn set_visible(&mut self, visible: bool) {
        self.suspended = !visible;
        if !visible {
            self.clear_controls();
        }
    }
}

impl FluxRun {
    fn on_key(&mut self, key: PhysicalKey, pressed: bool, repeat: bool) -> bool {
        let control = match key {
            PhysicalKey::ArrowUp | PhysicalKey::KeyW => Control::Up,
            PhysicalKey::ArrowRight | PhysicalKey::KeyD => Control::Right,
            PhysicalKey::ArrowDown | PhysicalKey::KeyS => Control::Down,
            PhysicalKey::ArrowLeft | PhysicalKey::KeyA => Control::Left,
            PhysicalKey::Space | PhysicalKey::KeyJ => Control::Magnet,
            PhysicalKey::Escape | PhysicalKey::KeyP | PhysicalKey::KeyT | PhysicalKey::Enter => {
                if pressed && !repeat {
                    match key {
                        PhysicalKey::KeyT => self.set_autoplay(self.game.mode != Mode::Auto),
                        PhysicalKey::Enter => match self.game.session {
                            Session::Title | Session::Results => self.start(Mode::Manual),
                            Session::Departed => self.next(),
                            Session::Paused => self.menu(),
                            Session::Running => (),
                        },
                        _ => self.menu(),
                    }
                }
                return true;
            }
            _ => return false,
        };
        set_button(&mut self.held, control, pressed);
        true
    }
}

fn set_button(state: &mut u8, control: Control, pressed: bool) {
    let bit = 1 << control as u8;
    if pressed {
        *state |= bit;
    } else {
        *state &= !bit;
    }
}

#[mirui::event]
fn key(event: event!(Key), model: model!(FluxRun)) -> bool {
    let Some(key) = event.physical else {
        return false;
    };
    let mut handled = false;
    model.update(|game| {
        handled = game.on_key(key, event.pressed, event.repeat == Some(true));
    });
    handled
}

#[mirui::event]
fn focus(event: event!(HostFocusChanged), model: model!(FluxRun)) {
    model.set_focus(event.focused);
}

#[mirui::event]
fn visibility(event: event!(HostVisibilityChanged), model: model!(FluxRun)) {
    model.set_visible(event.visible);
}

#[mirui::system(order = Animation)]
fn simulate(
    model: model!(FluxRun),
    delta: res!(DeltaTimeMs),
    theme: res!(Theme),
    audio: res!(optional mirui::audio::AudioHandle),
) {
    let tone = SceneTone::from_theme(&theme);
    if tone != model.tone() {
        model.set_tone(tone);
    }
    model.advance(delta.0);
    let cue = model.read(|state| state.game.cue);
    if let Some(audio) = audio.as_deref()
        && cue != Cue::None
    {
        audio.play(crate::presentation::sound::id(cue));
    }
}

/// Install the game with the desktop layout and canvas resolution policy.
pub fn setup_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    root: Entity,
) -> FluxRunHandle {
    install(app, root, false)
}

/// Install the same game with the Compact layout for a 128×128 display.
pub fn setup_compact_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    root: Entity,
) -> FluxRunHandle {
    install(app, root, true)
}

fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    root: Entity,
    compact: bool,
) -> FluxRunHandle {
    let tone = SceneTone::from_theme(app.theme());
    let model = app.add_model(FluxRun::new(root, compact, tone)).finish();
    app.add_plugin(SpriteStackPlugin);
    app.add_system(simulate::system(model.clone()));
    app.add_system(presentation::mount::system(model.clone()));
    app.on_event(Key::EVENT, key::callback(model.clone()));
    app.on_event(HostFocusChanged::EVENT, focus::callback(model.clone()));
    app.on_event(
        HostVisibilityChanged::EVENT,
        visibility::callback(model.clone()),
    );
    app.compose(root, build(model.clone(), compact));
    model
}

#[cfg(test)]
#[path = "../tests/unit/app.rs"]
mod tests;
