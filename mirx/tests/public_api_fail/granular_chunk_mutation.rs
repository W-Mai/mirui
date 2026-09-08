use mirx::{ChunkFlags, ChunkId, ChunkType, Document};
use mirx::extension::Policy;

fn mutate(document: &mut Document<'_>, id: ChunkId) {
    let mut chunk = document.get_mut(id).unwrap();
    chunk.set_type(ChunkType::META, Policy::infer()).unwrap();
    chunk.set_raw_policy(Policy::infer()).unwrap();
    chunk.replace_raw(b"payload", Policy::infer()).unwrap();
    chunk.set_flags(ChunkFlags::NONE, Policy::infer()).unwrap();
}

fn main() {}
