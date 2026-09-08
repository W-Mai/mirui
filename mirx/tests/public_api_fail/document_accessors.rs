use mirx::{ChunkId, Document};

fn inspect(document: &Document<'_>, id: ChunkId) {
    let _ = document.image(id);
    let _ = document.decode_font(id);
    let _ = document.decode_vector(id);
    let _ = document.meta(id);
    let _ = document.palette(id);
    let _ = document.frames(id);
}

fn main() {}
