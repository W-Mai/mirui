use mirx::coding::CodingTableError;

fn main() {
    let _ = CodingTableError::BufferTooSmall {
        needed: 12,
        available: 11,
    };
}
