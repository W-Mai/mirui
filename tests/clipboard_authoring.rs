use std::collections::VecDeque;

use mirui::core::cache::InspectCaches;
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{DisplayInfo, FramebufferAccess, InputEvent, Surface};
use mirui::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardFormat, ClipboardItem,
    ClipboardOutcome, ClipboardRead, ClipboardWrite, CutSelection, InspectSelection,
    PasteSelection, SelectionState,
};

const FORMATS: &[ClipboardFormat] = &[
    ClipboardFormat::borrowed("application/x-test-tile", &[3, 7]),
    ClipboardFormat::borrowed("text/plain", b"3,7"),
];
const ITEMS: &[ClipboardItem] = &[ClipboardItem::borrowed(FORMATS)];

struct ClipboardSurface {
    pixels: FramebufSurface<fn(&[u8], PhysicalRect)>,
    events: VecDeque<InputEvent>,
    write: Option<ClipboardWrite>,
    incoming: Option<ClipboardData>,
}

impl InspectCaches for ClipboardSurface {}

impl Surface for ClipboardSurface {
    fn display_info(&self) -> DisplayInfo {
        self.pixels.display_info()
    }

    fn flush(&mut self, area: PhysicalRect) {
        self.pixels.flush(area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }

    fn write_clipboard(&mut self, write: ClipboardWrite) -> ClipboardDispatch {
        assert!(self.write.replace(write).is_none());
        ClipboardDispatch::Pending
    }

    fn read_clipboard(&mut self, read: ClipboardRead) -> ClipboardDispatch {
        ClipboardDispatch::Complete(
            read.complete(self.incoming.take().ok_or(ClipboardError::Platform)),
        )
    }
}

impl FramebufferAccess for ClipboardSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.pixels.framebuffer()
    }
}

#[model]
struct TileData {
    value: [u8; 2],
    revision: u32,
    result: Option<Result<ClipboardOutcome, ClipboardError>>,
}

#[model]
impl TileData {
    fn value(&self) -> [u8; 2] {
        self.value
    }

    fn revision(&self) -> u32 {
        self.revision
    }

    fn replace(&mut self, value: [u8; 2]) {
        self.value = value;
        self.revision += 1;
    }

    fn finish(&mut self, result: Result<ClipboardOutcome, ClipboardError>) {
        self.result = Some(result);
    }

    fn result(&self) -> Option<Result<ClipboardOutcome, ClipboardError>> {
        self.result
    }
}

#[component]
struct Tile {
    data: model!(TileData),
}

#[event]
fn inspect(
    node: entity!(),
    _event: event!(InspectSelection),
    tiles: query!(Tile),
) -> Result<SelectionState, ClipboardError> {
    let tile = tiles.get(node).ok_or(ClipboardError::Unavailable)?;
    Ok(SelectionState::new(true, tile.data.revision()))
}

#[event]
fn cut(
    node: entity!(),
    _event: event!(CutSelection),
    tiles: query!(Tile),
) -> Result<(), ClipboardError> {
    let tile = tiles.get(node).ok_or(ClipboardError::Unavailable)?;
    tile.data.replace([0, 0]);
    Ok(())
}

#[view(component = Tile, input = (inspect::callback(), cut::callback()))]
fn tile_view() {}

#[model]
#[derive(Default)]
struct Drive {
    pending: Option<(Entity, Operation)>,
}

#[derive(Clone, Copy)]
enum Operation {
    Cut,
    Paste,
}

#[model]
impl Drive {
    fn cut(&mut self, target: Entity) {
        self.pending = Some((target, Operation::Cut));
    }

    fn paste(&mut self, target: Entity) {
        self.pending = Some((target, Operation::Paste));
    }

    fn pending(&self) -> Option<(Entity, Operation)> {
        self.pending
    }

    fn clear(&mut self) {
        self.pending = None;
    }
}

#[system]
fn drive_clipboard(drive: model!(Drive), commands: commands!()) {
    if let Some((target, operation)) = drive.pending() {
        drive.clear();
        match operation {
            Operation::Cut => commands.clipboard().cut(target),
            Operation::Paste => commands.clipboard().paste(target),
        }
    }
}

#[compose]
fn tile(data: model!(TileData)) -> Entity {
    ui! {
        View(width: 8, height: 8) [Tile { data: data.clone() }]
        on CopySelection {
            assert_eq!(data.value(), [3, 7]);
            Ok(ClipboardData::borrowed(ITEMS))
        }
        on PasteSelection(|event: event!(PasteSelection), node: entity!(), tiles: query!(Tile)| {
            let [item] = event.data().items() else {
                return Err(ClipboardError::Unsupported);
            };
            let format = item.formats().iter()
                .find(|format| format.mime() == "application/x-test-tile")
                .ok_or(ClipboardError::Unsupported)?;
            let value = <[u8; 2]>::try_from(format.bytes())
                .map_err(|_| ClipboardError::InvalidData)?;
            let tile = tiles.get(node).ok_or(ClipboardError::Unavailable)?;
            tile.data.replace(value);
            Ok(())
        })
        on ClipboardFinished {
            data.finish(event.result);
        }
    }
}

#[test]
fn external_component_requests_share_public_authoring_and_platform_boundaries() {
    let surface = ClipboardSurface {
        pixels: FramebufSurface::new(8, 8, |_, _| {}),
        events: VecDeque::new(),
        write: None,
        incoming: None,
    };
    let mut app = App::new(surface);
    app.with_widget(tile_view::view());
    let data = app.add_model(TileData {
        value: [3, 7],
        revision: 0,
        result: None,
    });
    let drive = app.add_model(Drive::default());
    app.add_system(drive_clipboard::system(drive.clone()));
    let root = app.spawn_root().id();
    let node = app.compose(root, tile(data.clone()));

    drive.cut(node);
    assert!(!app.tick());
    assert!(!app.tick());
    let write = app.backend.write.take().expect("pending platform write");
    assert_eq!(write.data().items()[0].formats().len(), 2);
    assert_eq!(data.value(), [3, 7]);
    assert_eq!(data.result(), None);
    app.backend
        .events
        .push_back(InputEvent::Clipboard(write.complete(Ok(()))));
    assert!(!app.tick());
    assert_eq!(data.value(), [0, 0]);
    assert_eq!(data.result(), Some(Ok(ClipboardOutcome::Cut)));

    app.backend.incoming = Some(ClipboardData::borrowed(ITEMS));
    drive.paste(node);
    assert!(!app.tick());
    assert!(!app.tick());
    assert_eq!(data.value(), [3, 7]);
    assert_eq!(data.result(), Some(Ok(ClipboardOutcome::Pasted)));

    drive.cut(node);
    assert!(!app.tick());
    assert!(!app.tick());
    let write = app.backend.write.take().expect("second platform write");
    data.replace([9, 9]);
    data.replace([3, 7]);
    app.backend
        .events
        .push_back(InputEvent::Clipboard(write.complete(Ok(()))));
    assert!(!app.tick());
    assert_eq!(data.value(), [3, 7]);
    assert_eq!(data.result(), Some(Ok(ClipboardOutcome::CopiedWithoutCut)));

    drive.cut(node);
    assert!(!app.tick());
    assert!(!app.tick());
    let write = app.backend.write.take().expect("rejected platform write");
    app.backend.events.push_back(InputEvent::Clipboard(
        write.complete(Err(ClipboardError::PermissionDenied)),
    ));
    assert!(!app.tick());
    assert_eq!(data.value(), [3, 7]);
    assert_eq!(data.result(), Some(Err(ClipboardError::PermissionDenied)));

    app.backend.incoming = Some(ClipboardData::text("not a tile").unwrap());
    drive.paste(node);
    assert!(!app.tick());
    assert!(!app.tick());
    assert_eq!(data.value(), [3, 7]);
    assert_eq!(data.result(), Some(Err(ClipboardError::Unsupported)));
}
