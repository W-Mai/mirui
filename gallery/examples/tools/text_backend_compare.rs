use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use gallery::backend_parity::{HEIGHT, STABLE_TEXT_BOTTOM, WIDTH};

struct Image {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

const MAX_STABLE_RMSE: f64 = 0.01;
const MIN_FOREGROUND_IOU: f64 = 0.6;
const TEXTURE_COLORS: [[u8; 3]; 6] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
    [255, 0, 255],
];

fn read(path: &Path) -> Image {
    let decoder = png::Decoder::new(BufReader::new(File::open(path).expect("open PNG")));
    let mut reader = decoder.read_info().expect("read PNG header");
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut bytes = vec![0; reader.output_buffer_size().expect("PNG buffer size")];
    let info = reader.next_frame(&mut bytes).expect("decode PNG");
    bytes.truncate(info.buffer_size());
    Image {
        width: info.width,
        height: info.height,
        bytes,
    }
}

fn rgb_rmse(reference: &Image, candidate: &Image, rows: u32) -> f64 {
    let row_bytes = reference.width as usize * 4;
    let end = rows.min(reference.height) as usize * row_bytes;
    let mut squared = 0u64;
    let mut samples = 0u64;
    for (left, right) in reference.bytes[..end]
        .chunks_exact(4)
        .zip(candidate.bytes[..end].chunks_exact(4))
    {
        for channel in 0..3 {
            let delta = left[channel].abs_diff(right[channel]);
            squared += u64::from(delta) * u64::from(delta);
            samples += 1;
        }
    }
    (squared as f64 / samples as f64).sqrt() / 255.0
}

fn foreground_iou(reference: &Image, candidate: &Image) -> f64 {
    let foreground = |pixel: &[u8]| {
        pixel[0].abs_diff(16) > 12 || pixel[1].abs_diff(18) > 12 || pixel[2].abs_diff(27) > 12
    };
    let mut union = 0u64;
    let mut intersection = 0u64;
    for (left, right) in reference
        .bytes
        .chunks_exact(4)
        .zip(candidate.bytes.chunks_exact(4))
    {
        let left = foreground(left);
        let right = foreground(right);
        union += u64::from(left || right);
        intersection += u64::from(left && right);
    }
    intersection as f64 / union as f64
}

fn verify_texture_colors(image: &Image, scale: u32) {
    for texture in 0..3 {
        for (index, expected) in TEXTURE_COLORS.iter().enumerate() {
            let x = (68 + texture * 200 + index as u32 % 3 * 40) * scale;
            let y = (264 + index as u32 / 3 * 40) * scale;
            let offset = ((y * image.width + x) * 4) as usize;
            assert_eq!(&image.bytes[offset..offset + 3], expected);
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let reference = read(Path::new(&args.next().expect("reference PNG")));
    let candidate_path = args.next().expect("candidate PNG");
    let candidate = read(Path::new(&candidate_path));
    assert_eq!(
        (candidate.width, candidate.height),
        (reference.width, reference.height),
        "image dimensions differ"
    );
    let scale = reference.width / u32::from(WIDTH);
    assert!(scale > 0);
    assert_eq!(reference.width, u32::from(WIDTH) * scale);
    assert_eq!(reference.height, u32::from(HEIGHT) * scale);
    verify_texture_colors(&reference, scale);
    verify_texture_colors(&candidate, scale);

    let mut maximum = 0u8;
    for (left, right) in reference
        .bytes
        .chunks_exact(4)
        .zip(candidate.bytes.chunks_exact(4))
    {
        for channel in 0..3 {
            let delta = left[channel].abs_diff(right[channel]);
            maximum = maximum.max(delta);
        }
    }
    let stable_rmse = rgb_rmse(
        &reference,
        &candidate,
        u32::from(STABLE_TEXT_BOTTOM) * scale,
    );
    let iou = foreground_iou(&reference, &candidate);
    println!(
        "{candidate_path}: stable RGB RMSE={stable_rmse:.7}, max={maximum}, foreground IoU={iou:.7}"
    );
    assert!(
        stable_rmse <= MAX_STABLE_RMSE,
        "stable coverage/SDF region exceeds RGB RMSE {MAX_STABLE_RMSE}"
    );
    assert!(
        iou >= MIN_FOREGROUND_IOU,
        "affine glyph placement falls below foreground IoU {MIN_FOREGROUND_IOU}"
    );
}
