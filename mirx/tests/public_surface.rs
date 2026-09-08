fn public_root_declarations(source: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut current = None::<String>;

    for line in source.lines() {
        if let Some(statement) = &mut current {
            statement.push(' ');
            statement.push_str(line.trim());
            if line.contains(';') {
                declarations.push(statement.split_whitespace().collect::<Vec<_>>().join(" "));
                current = None;
            }
            continue;
        }

        if !line.starts_with("pub ") {
            continue;
        }
        assert!(line.starts_with("pub mod ") || line.starts_with("pub use "));
        if line.contains(';') {
            declarations.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
        } else {
            current = Some(line.trim().to_owned());
        }
    }

    assert!(current.is_none());
    declarations
}

#[test]
fn root_exports_match_the_canonical_surface() {
    assert_eq!(
        public_root_declarations(include_str!("../src/lib.rs")),
        [
            "pub mod coding;",
            "pub mod document;",
            "pub mod extension;",
            "pub mod font;",
            "pub mod frames;",
            "pub mod image;",
            "pub mod meta;",
            "pub mod palette;",
            "pub mod reader;",
            "pub mod scene;",
            "pub mod types;",
            "pub use document::{Document, DocumentChunkMut, DocumentChunkRef};",
            "pub use header::Layout;",
            "pub use model::{ChunkFlags, ChunkId, ChunkType, InvalidChunkType, PrimaryHints};",
            "pub use reader::{ChunkRef, Reader};",
        ]
    );
}
