use mirx::{
    font::FontView,
    frames::FramesView,
    image::{EncodedImageView, RawImageView},
};

fn font(view: FontView<'_>) {
    let _ = view.media();
}

fn frames(view: FramesView<'_>) {
    let _ = view.media();
}

fn raw_image(view: RawImageView<'_>) {
    let _ = view.media();
}

fn encoded_image(view: EncodedImageView<'_>) {
    let _ = view.media();
}

fn main() {}
