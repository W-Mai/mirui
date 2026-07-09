mod vector;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn cmd_gen_mirx(args: &[String]) -> Result {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "vector" => vector::run(&args[1..]),
        "font" | "bundle" => {
            eprintln!(
                "gen-mirx font/bundle moved to the icu tool:\n\
                 \n  icu bake-font <ttf> --charset <chars> --size <px> \\\n\
                       --bit-depth <1|2|4|8> --format <sdf|gray> -O <out-dir>\n\
                 \n  icu merge-fonts <a.mirx> <b.mirx> ... -O <bundle.mirx>\n\
                 \n\
                 install: cargo install icu_tool  (or brew install w-mai/homebrew-cellar/icu_tool)"
            );
            std::process::exit(1);
        }
        _ => {
            eprintln!(
                "usage: cargo xtask gen-mirx vector --in <scene.txt> --out <scene.mirx>\n\
                 \n\
                 font/bundle live in the icu tool now:\n\
                   icu bake-font ...\n\
                   icu merge-fonts ..."
            );
            std::process::exit(1);
        }
    }
}
