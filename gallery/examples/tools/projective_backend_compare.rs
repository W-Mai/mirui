use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

struct Image {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

const MIN_FOREGROUND_IOU: f64 = 0.975;
const MAX_EDGE_OUTLIER_RATE: f64 = 0.0005;

impl Image {
    fn read(path: &Path) -> Self {
        let decoder = png::Decoder::new(BufReader::new(File::open(path).expect("open PNG")));
        let mut reader = decoder.read_info().expect("read PNG header");
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
        let mut pixels = vec![0; reader.output_buffer_size().expect("PNG buffer size")];
        let frame = reader.next_frame(&mut pixels).expect("decode PNG");
        pixels.truncate(frame.buffer_size());
        Self {
            width: frame.width as usize,
            height: frame.height as usize,
            pixels,
        }
    }

    fn foreground(&self) -> Vec<bool> {
        let background = &self.pixels[..3];
        self.pixels
            .chunks_exact(4)
            .map(|pixel| (0..3).any(|channel| pixel[channel].abs_diff(background[channel]) > 12))
            .collect()
    }
}

fn near(mask: &[bool], width: usize, height: usize, x: usize, y: usize) -> bool {
    let x0 = x.saturating_sub(2);
    let y0 = y.saturating_sub(2);
    let x1 = (x + 2).min(width - 1);
    let y1 = (y + 2).min(height - 1);
    (y0..=y1).any(|row| (x0..=x1).any(|col| mask[row * width + col]))
}

fn compare(reference: &Image, candidate: &Image) -> (f64, f64, f64) {
    assert_eq!(
        (reference.width, reference.height),
        (candidate.width, candidate.height)
    );
    let reference_mask = reference.foreground();
    let candidate_mask = candidate.foreground();
    let mut intersection = 0u64;
    let mut union = 0u64;
    let mut reference_count = 0u64;
    let mut candidate_count = 0u64;
    let mut unmatched = 0u64;
    let mut squared = 0u64;
    let mut color_samples = 0u64;

    for y in 0..reference.height {
        for x in 0..reference.width {
            let index = y * reference.width + x;
            let a = reference_mask[index];
            let b = candidate_mask[index];
            intersection += u64::from(a && b);
            union += u64::from(a || b);
            reference_count += u64::from(a);
            candidate_count += u64::from(b);
            if a && !near(&candidate_mask, reference.width, reference.height, x, y) {
                unmatched += 1;
            }
            if b && !near(&reference_mask, reference.width, reference.height, x, y) {
                unmatched += 1;
            }
            if a && b && x > 1 && y > 1 && x + 2 < reference.width && y + 2 < reference.height {
                let interior = (y - 1..=y + 1).all(|row| {
                    (x - 1..=x + 1).all(|col| {
                        let neighbor = row * reference.width + col;
                        reference_mask[neighbor] && candidate_mask[neighbor]
                    })
                });
                if interior {
                    for channel in 0..3 {
                        let delta = reference.pixels[index * 4 + channel]
                            .abs_diff(candidate.pixels[index * 4 + channel]);
                        squared += u64::from(delta) * u64::from(delta);
                        color_samples += 1;
                    }
                }
            }
        }
    }

    assert!(union > 0);
    let iou = intersection as f64 / union as f64;
    let outliers = unmatched as f64 / (reference_count + candidate_count) as f64;
    let interior_rmse = if color_samples == 0 {
        1.0
    } else {
        (squared as f64 / color_samples as f64).sqrt() / 255.0
    };
    (iou, outliers, interior_rmse)
}

fn main() {
    let mut args = env::args().skip(1);
    let reference = Image::read(Path::new(&args.next().expect("reference PNG")));
    let candidate = Image::read(Path::new(&args.next().expect("candidate PNG")));
    let (iou, outliers, interior_rmse) = compare(&reference, &candidate);
    println!(
        "foreground IoU={iou:.6}, edge outliers beyond 2 px={outliers:.6}, interior RGB RMSE={interior_rmse:.6}"
    );
    assert!(iou >= MIN_FOREGROUND_IOU, "projected silhouette differs");
    assert!(
        outliers <= MAX_EDGE_OUTLIER_RATE,
        "projected geometry escapes its reference bounds"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x_start: usize) -> Image {
        let mut pixels = [16u8, 18, 27, 255].repeat(16 * 16);
        for y in 4..8 {
            for x in x_start..x_start + 4 {
                pixels[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4].copy_from_slice(&[255, 30, 20, 255]);
            }
        }
        Image {
            width: 16,
            height: 16,
            pixels,
        }
    }

    #[test]
    fn matching_geometry_has_no_outliers() {
        let reference = sample(2);
        let (iou, outliers, color) = compare(&reference, &sample(2));
        assert_eq!((iou, outliers, color), (1.0, 0.0, 0.0));
    }

    #[test]
    fn distant_geometry_fails_the_silhouette_gate() {
        let (iou, outliers, _) = compare(&sample(2), &sample(10));
        assert!(iou < MIN_FOREGROUND_IOU);
        assert!(outliers > MAX_EDGE_OUTLIER_RATE);
    }
}
