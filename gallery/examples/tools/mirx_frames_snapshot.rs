//! Encodes a compressed MIRX timeline, binds fixed aligned playback storage,
//! and writes three tick-selected frames as one PPM contact sheet.

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use mirui::render::mirx_frames::MirxFramesPlan;
use mirui::render::texture::MirxTextureOptions;
use mirx::image::{CacheSync, ColorDescription, MemoryPlacement, SampleLayout, SurfaceDescriptor};
use mirx::{Document, FrameEncodingSet, FrameSequence, FramesEncoder};

const WIDTH: usize = 64;
const HEIGHT: usize = 48;
const SAMPLE_TICKS: [u64; 3] = [0, 100, 350];

#[repr(align(64))]
struct Aligned<const N: usize>([u8; N]);

fn frame_samples(frame: usize) -> Vec<u8> {
    let (base, accent, left) = [
        ([18u8, 31, 52], [64u8, 139, 245], 6usize),
        ([16u8, 48, 42], [46u8, 196, 126], 24usize),
        ([48u8, 25, 54], [221u8, 92, 197], 42usize),
    ][frame];
    let mut samples = vec![0; WIDTH * HEIGHT * 3];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let shade = ((x * 2 + y * 3) % 48) as u8;
            let inside = (left..left + 16).contains(&x) && (15..31).contains(&y);
            let color = if inside {
                accent
            } else {
                [
                    base[0].saturating_add(shade / 3),
                    base[1].saturating_add(shade / 2),
                    base[2].saturating_add(shade),
                ]
            };
            let offset = (y * WIDTH + x) * 3;
            samples[offset..offset + 3].copy_from_slice(&color);
        }
    }
    samples
}

fn encode_timeline() -> Vec<u8> {
    let surface = SurfaceDescriptor::new(
        WIDTH as u32,
        HEIGHT as u32,
        SampleLayout::RGB888,
        ColorDescription::SRGB,
    )
    .expect("valid RGB surface");
    let sequence = FrameSequence::new(3, 1_000, 100)
        .expect("valid sequence")
        .with_play_count(2)
        .with_max_delta_frames(2)
        .expect("valid recovery bound");
    let mut encoder = FramesEncoder::new(sequence, surface)
        .expect("valid frame encoder")
        .with_profiles(FrameEncodingSet::lossless())
        .expect("valid lossless profiles")
        .with_input_alignment(64)
        .expect("valid input alignment");
    for (frame, duration) in [100, 250, 150].into_iter().enumerate() {
        encoder
            .push_with_duration(&frame_samples(frame), duration)
            .expect("encode frame");
    }

    let mut document = Document::new();
    let id = document
        .push_frames(encoder.finish().expect("finish frame payload"))
        .expect("insert frame payload");
    document.set_primary(id).expect("select frame timeline");
    document
        .encode(&Default::default())
        .expect("encode MIRX document")
}

fn output_path() -> PathBuf {
    env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        let directory =
            env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".local/screenshots".to_owned());
        std::fs::create_dir_all(&directory).expect("create snapshot directory");
        PathBuf::from(directory).join("mirx_frames_timeline.ppm")
    })
}

fn main() {
    let bytes = encode_timeline();
    let options = MirxTextureOptions::new()
        .with_input_memory(MemoryPlacement::Flash)
        .with_output_memory(MemoryPlacement::SharedNoncoherent)
        .with_workspace_memory(MemoryPlacement::SharedCoherent)
        .with_workspace_alignment(64)
        .with_base_alignment(64)
        .with_plane_alignment(64)
        .with_width_multiple(64)
        .with_stride_multiple(64);
    let mut groups = [None];
    let plan = MirxFramesPlan::open(&bytes, options, &mut groups).expect("preflight timeline");
    assert_eq!(plan.frame_count(), 3);
    assert_eq!(plan.timeline().cycle_duration_ticks(), 500);
    assert_eq!(plan.group_workspace_len(), groups.len());
    assert_eq!(plan.decode_request(), options.decode_request());
    assert_eq!(plan.input_sync(), CacheSync::None);
    assert_eq!(plan.output_sync(), CacheSync::CleanAfterWrite);
    assert_eq!(plan.input_alignment(), 64);

    let mut canvas = Aligned([0; WIDTH * HEIGHT * 3]);
    let mut workspace = Aligned([0; WIDTH * HEIGHT * 4]);
    assert!(plan.canvas_requirements().byte_len() <= canvas.0.len());
    assert!(plan.workspace_requirements().byte_len() <= workspace.0.len());
    assert_eq!(plan.backup_requirements().byte_len(), 0);
    let mut session = plan
        .bind(&mut groups, &mut canvas.0, &mut workspace.0, &mut [])
        .expect("bind fixed playback storage");

    let sheet_width = WIDTH * SAMPLE_TICKS.len();
    let sheet_stride = sheet_width * 3;
    let mut sheet = vec![0; sheet_stride * HEIGHT];
    let mut canvas_address = None;
    for (sample, elapsed_ticks) in SAMPLE_TICKS.into_iter().enumerate() {
        let (position, texture) = session
            .present_at(elapsed_ticks)
            .expect("present selected tick")
            .expect("tick is inside finite timeline");
        assert_eq!(position.frame(), sample as u32);
        let address = texture.buf.as_slice().as_ptr() as usize;
        assert_eq!(address % 64, 0);
        assert!(canvas_address.is_none_or(|previous| previous == address));
        canvas_address = Some(address);

        for row in 0..HEIGHT {
            let source_start = row * texture.stride;
            let source_end = source_start + WIDTH * 3;
            let target_start = row * sheet_stride + sample * WIDTH * 3;
            sheet[target_start..target_start + WIDTH * 3]
                .copy_from_slice(&texture.buf.as_slice()[source_start..source_end]);
        }
    }

    let path = output_path();
    let file = File::create(&path).expect("create snapshot");
    let mut output = BufWriter::new(file);
    write!(output, "P6\n{sheet_width} {HEIGHT}\n255\n").expect("write PPM header");
    output.write_all(&sheet).expect("write PPM samples");
    output.flush().expect("flush snapshot");

    eprintln!(
        "saved {} ({} MIRX bytes, {}-byte canvas, {}-byte workspace, address/stride 64-byte aligned, host input aligned: {}, output cache action: {:?})",
        path.display(),
        bytes.len(),
        plan.canvas_requirements().byte_len(),
        plan.workspace_requirements().byte_len(),
        plan.input_addresses_are_aligned(),
        plan.output_sync(),
    );
}
