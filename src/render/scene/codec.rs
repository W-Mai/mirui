use alloc::vec::Vec;

use crate::render::scene::SceneOp;

pub use mirx::scene::CodecError;

pub fn encode_scene(ops: &[SceneOp]) -> Result<Vec<u8>, CodecError> {
    let mirx_ops: alloc::vec::Vec<mirx::scene::SceneOp> =
        ops.iter().cloned().map(Into::into).collect();
    mirx::scene::Scene::from_ops(mirx_ops).encode()
}

pub fn decode_scene(payload: &[u8]) -> Result<Vec<SceneOp>, CodecError> {
    let scene = mirx::scene::Scene::decode(payload)?;
    Ok(scene.ops.into_iter().map(Into::into).collect())
}

#[cfg(test)]
#[path = "../../../tests/unit/render/scene/codec.rs"]
mod tests;
