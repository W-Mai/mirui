fn main() {
    let (w, h) = gallery::demos::cover_flow::SIZE;
    gallery::run(
        "mirui - cover flow",
        w,
        h,
        gallery::demos::cover_flow::build,
    );
}
