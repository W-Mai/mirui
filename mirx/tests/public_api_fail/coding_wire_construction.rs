use mirx::{
    coding::{CodingRecord, CodingTable},
    image::{EncodedImageAsset, SurfaceDescriptor},
};

fn retained<'a>(surface: SurfaceDescriptor, table: CodingTable<'a>, data: &'a [u8]) {
    let _ = EncodedImageAsset::from_codings(surface, table, data);
}

fn main() {
    let mut bytes = [0; 12];
    let _ = CodingTable::open(&bytes);
    let _ = CodingTable::encoded_len(&[CodingRecord::RAW]);
    let _ = CodingTable::encode_into(&[CodingRecord::RAW], &mut bytes);
}
