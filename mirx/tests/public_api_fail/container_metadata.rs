use mirx::{Document, Reader};

fn inspect(reader: &Reader<'_>, document: &Document<'_>) {
    let _ = reader.version_major();
    let _ = reader.version_minor();
    let _ = reader.file_flags();
    let _ = document.file_metadata();
}

fn main() {}
