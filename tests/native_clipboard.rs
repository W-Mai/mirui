extern crate alloc;

#[cfg(all(target_os = "macos", any(feature = "sdl", feature = "wgpu")))]
pub mod ui {
    pub mod clipboard {
        pub use mirui::ui::clipboard::ClipboardError;
        mod data {
            include!("../src/ui/clipboard/data.rs");
        }
        pub use data::{ClipboardData, ClipboardFormat, ClipboardItem};
    }
}

#[cfg(all(target_os = "macos", any(feature = "sdl", feature = "wgpu")))]
mod native {
    include!("../src/surface/host_clipboard/macos.rs");

    mod contracts {
        include!("native_clipboard/contracts.rs");
    }

    pub fn verify() {
        MainThreadMarker::new().expect("native clipboard contracts run on the main thread");
        assert_eq!(
            std::thread::spawn(|| write(&ClipboardData::text("unused").unwrap()))
                .join()
                .unwrap(),
            Err(ClipboardError::Unavailable)
        );
        assert!(matches!(
            std::thread::spawn(read).join().unwrap(),
            Err(ClipboardError::Unavailable)
        ));
        contracts::native_item_and_representation_boundaries_round_trip();
        contracts::invalid_representation_does_not_replace_native_contents();
        contracts::encoded_images_and_native_bitmap_import_share_the_png_representation();
        contracts::uri_list_keeps_bytes_and_offers_native_file_paths();
        contracts::rtf_aliases_preserve_distinct_authored_representations();
        println!("macOS clipboard: main-thread boundary and five native format contracts passed");
    }
}

fn main() {
    if !std::env::args().any(|arg| arg == "--ignored") {
        println!("native clipboard contracts skipped; run with --ignored on a desktop session");
        return;
    }
    #[cfg(all(target_os = "macos", any(feature = "sdl", feature = "wgpu")))]
    native::verify();
    #[cfg(not(all(target_os = "macos", any(feature = "sdl", feature = "wgpu"))))]
    panic!("native clipboard contracts require macOS and the sdl or wgpu feature");
}
