fn main() {
    let (w, h) = gallery::demos::nested_scroll::SIZE;
    gallery::run(
        "mirui - nested scroll",
        w,
        h,
        gallery::demos::nested_scroll::build,
    );
}
