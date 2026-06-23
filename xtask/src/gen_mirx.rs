//! `cargo xtask gen-mirx <subcmd>` — host-side asset packaging.
//!
//! Subcommands:
//!
//!   font    Build a `chunk_type::FONT = 0x0004` atlas (SDF or
//!           grayscale) from a TTF/OTF plus a charset.
//!   bundle  Merge several single-font `.mirx` files into one
//!           multi-representation bundle.

mod bundle;
mod font;
mod vector;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn cmd_gen_mirx(args: &[String]) -> Result {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "font" => font::run(&args[1..]),
        "bundle" => bundle::run(&args[1..]),
        "vector" => vector::run(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  cargo xtask gen-mirx font --ttf <f.ttf> --charset <chars> --size <px> \\\n    --bit-depth <1|2|4|8> --format <sdf|gray> --out <atlas.mirx>\n  cargo xtask gen-mirx bundle <a.mirx> <b.mirx> ... --out <bundle.mirx>\n  cargo xtask gen-mirx vector --in <scene.txt> --out <scene.mirx>"
            );
            std::process::exit(1);
        }
    }
}
