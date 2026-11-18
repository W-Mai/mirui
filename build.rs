#[cfg(feature = "gallery")]
#[path = "build/pixel_flight.rs"]
mod pixel_flight;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "gallery")]
    {
        println!("cargo:rerun-if-changed=build/pixel_flight.rs");
        println!("cargo:rerun-if-changed=src/gallery/demos/assets/jet.mirx");
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        pixel_flight::prepare(include_bytes!("src/gallery/demos/assets/jet.mirx"), &output)
            .expect("invalid Pixel Flight jet.mirx or unwritable build output");
    }
}
