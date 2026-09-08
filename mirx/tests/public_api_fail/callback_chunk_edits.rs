use mirx::document::TryEditError;
use mirx::{ChunkId, Document};

fn edit_meta(document: &mut Document<'_>, id: ChunkId) {
    document
        .get_mut(id)
        .unwrap()
        .edit_meta(|_| {})
        .unwrap();
}

fn try_edit_meta(document: &mut Document<'_>, id: ChunkId) {
    document
        .get_mut(id)
        .unwrap()
        .try_edit_meta(|_| Ok::<_, ()>(()))
        .unwrap();
}

fn main() {}
