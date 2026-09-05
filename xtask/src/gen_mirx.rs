mod bundle;
mod font;
mod frames;
mod image;
mod vector;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn cmd_gen_mirx(args: &[String]) -> Result {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "font" => font::run(&args[1..]),
        "frames" => frames::run(&args[1..]),
        "image" => image::run(&args[1..]),
        "bundle" => bundle::run(&args[1..]),
        "vector" => vector::run(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  \
                 cargo xtask gen-mirx font --ttf <f.ttf> --charset <s> \\\n    \
                     --size <px> --bit-depth <1|2|4|8> --format <sdf|gray> --out <atlas.mirx>\n  \
                 cargo xtask gen-mirx image --in <image> --out <image.mirx> \
    \
                     [--format <rgba8888|rgb888|rgb565|rgb565-swapped|bgra8888|xrgb8888|i1|i2|i4|i8>] \
    \
                     [--coding <raw|pixel|rle|lz4|frequency-reversible|frequency-quantized>] \
                     [--quality <1..100>] [--stride-align <raw-row-bytes>]\n  \
                 cargo xtask gen-mirx frames --in <frame> --in <frame> --out <animation.mirx> \
    \
                     [--format <rgba8888|rgb888|i1|i2|i4|i8>] [--duration <ticks>] \
    \
                     [--timebase <ticks-per-second>] [--max-delta-frames <count>] \
    \
                     [--tile <width>x<height>|none] [--input-align <bytes>] [--quality <1..100>]\n  \
                 cargo xtask gen-mirx bundle <a.mirx> <b.mirx> ... --out <bundle.mirx>\n  \
                 cargo xtask gen-mirx vector --in <scene.txt> --out <scene.mirx>\n\
                 \n\
                 font/image/frames/bundle use the icu tool for source decoding.\n\
                 install: cargo install icu_tool  \
                 (or brew install w-mai/homebrew-cellar/icu_tool)"
            );
            std::process::exit(1);
        }
    }
}

fn probe_icu() -> Result {
    use std::process::Command;
    if Command::new(icu_program())
        .arg("--version")
        .output()
        .is_err()
    {
        return Err("icu binary not found on PATH.\n  \
             install: cargo install icu_tool  \
             (or brew install w-mai/homebrew-cellar/icu_tool)"
            .into());
    }
    Ok(())
}

fn icu_program() -> String {
    std::env::var("MIRU_ICU_BIN").unwrap_or_else(|_| "icu".into())
}
