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
                "usage:\n  \
                 cargo xtask gen-mirx font --ttf <f.ttf> --charset <s> \\\n    \
                     --size <px> --bit-depth <1|2|4|8> --format <sdf|gray> --out <atlas.mirx>\n  \
                 cargo xtask gen-mirx bundle <a.mirx> <b.mirx> ... --out <bundle.mirx>\n  \
                 cargo xtask gen-mirx vector --in <scene.txt> --out <scene.mirx>\n\
                 \n\
                 font/bundle are shims around the icu tool (icu bake-font / icu merge-fonts).\n\
                 install: cargo install icu_tool  \
                 (or brew install w-mai/homebrew-cellar/icu_tool)"
            );
            std::process::exit(1);
        }
    }
}

fn probe_icu() -> Result {
    use std::process::Command;
    if Command::new("icu").arg("--version").output().is_err() {
        return Err("icu binary not found on PATH.\n  \
             install: cargo install icu_tool  \
             (or brew install w-mai/homebrew-cellar/icu_tool)"
            .into());
    }
    Ok(())
}
