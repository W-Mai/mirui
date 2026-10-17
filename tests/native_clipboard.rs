extern crate alloc;

#[cfg(all(
    any(target_os = "macos", target_os = "windows"),
    any(feature = "sdl", feature = "wgpu")
))]
pub mod ui {
    pub mod clipboard {
        pub use mirui::ui::clipboard::ClipboardError;
        mod data {
            include!("../src/ui/clipboard/data.rs");
        }
        #[cfg(target_os = "windows")]
        pub(crate) use data::validate_mime;
        pub use data::{ClipboardData, ClipboardFormat, ClipboardItem};
    }
}

#[cfg(all(target_os = "windows", any(feature = "sdl", feature = "wgpu")))]
#[allow(
    dead_code,
    unused_imports,
    reason = "the main-thread harness does not register included libtest cases"
)]
mod native {
    include!("../src/surface/host_clipboard/windows/mod.rs");

    mod contracts {
        include!("native_clipboard/windows.rs");
    }

    pub fn verify() {
        contracts::image_and_file_conversions();
        println!("Windows clipboard: native image and file conversion contracts passed");
        if std::env::args().any(|arg| arg == "--replace-system-clipboard") {
            contracts::system_exchange();
            println!("Windows clipboard: system exchange contracts passed; clipboard replaced");
        } else {
            println!(
                "system exchange skipped; --replace-system-clipboard replaces existing contents"
            );
        }
    }
}

#[cfg(all(target_os = "macos", any(feature = "sdl", feature = "wgpu")))]
#[path = "../src/surface/host_clipboard/apple.rs"]
mod apple;

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
    #[cfg(all(
        any(target_os = "macos", target_os = "windows"),
        any(feature = "sdl", feature = "wgpu")
    ))]
    native::verify();
    #[cfg(not(all(
        any(target_os = "macos", target_os = "windows"),
        any(feature = "sdl", feature = "wgpu")
    )))]
    panic!("native clipboard contracts require macOS or Windows and the sdl or wgpu feature");
}
