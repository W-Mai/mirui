//! `cargo xtask gen-mirx <subcmd>` — host-side asset packaging.
//!
//! Subcommands:
//!
//!   font    Build a `chunk_type::FONT = 0x0004` SDF atlas from a TTF
//!           or OTF file plus a charset list. The output is a `.mirx`
//!           file ready to `include_bytes!` in a runtime crate.

mod font;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn cmd_gen_mirx(args: &[String]) -> Result {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "font" => font::run(&args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask gen-mirx font \\\n  --ttf <font.ttf> --charset <chars> --size <px> \\\n  --bit-depth <4|8> --spread <px> --out <atlas.mirx>\n\nor --charset-file <path> for a UTF-8 file (one line is fine)."
            );
            std::process::exit(1);
        }
    }
}
