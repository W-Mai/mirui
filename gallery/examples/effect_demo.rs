fn main() {
    let (w, h) = gallery::demos::effect::SIZE;
    gallery::run(
        "mirui — effect widget demo",
        w,
        h,
        gallery::demos::effect::build,
    );
}
