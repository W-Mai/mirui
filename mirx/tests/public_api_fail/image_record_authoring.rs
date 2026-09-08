use mirx::image::{
    ATLAS_REGION_LEN, AtlasMap, PLANE_RECORD_LEN, PlaneMemoryLayout, SURFACE_RECORD_LEN,
    SurfaceDescriptor,
};

fn main() {
    let _ = AtlasMap::encoded_len;
    let _ = AtlasMap::encode_into;
    let _ = PlaneMemoryLayout::from_record;
    let _ = PlaneMemoryLayout::encode_record_into;
    let _ = SurfaceDescriptor::from_record;
    let _ = SurfaceDescriptor::encode_record_into;
    let _ = (ATLAS_REGION_LEN, PLANE_RECORD_LEN, SURFACE_RECORD_LEN);
}
