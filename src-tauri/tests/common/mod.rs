// src-tauri/tests/common/mod.rs
//
// Shared fixtures for the pipeline battery: synthetic charts, a shared GPU
// device, and harnesses for the two shaders that production code does not
// expose directly (calcParams readback and histogram.wgsl).
//
// A bug in a fixture is the worst kind of bug, so everything here is kept
// deliberately simple and is itself covered by tests in `image_prep.rs`.

#![allow(dead_code)]

use std::sync::OnceLock;

use image::{ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

use open_darkroom_lib::color::{build_srgb_to_linear_lut_u16, linearize_u16};
use open_darkroom_lib::export_rendering::{
    render_image, sliders_to_bytes, view_to_bytes, SlidersPayload, ViewPayload, PARAMS_BYTES,
    SHARP_PARAMS_BYTES,
};
use open_darkroom_lib::image_opening::{Rgba16Image, HIST_BINS};

// ── Source files under contract ───────────────────────────────────────────────
//
// The drift guards in `shader_contracts.rs` read these as text. They live here
// so every path is declared in exactly one place.

pub const HISTOGRAM_WGSL: &str = include_str!("../../../src/lib/gpu/shaders/histogram.wgsl");
pub const CALC_PARAMS_PIPELINE_TS: &str =
    include_str!("../../../src/lib/gpu/pipelines/calcParamsPipeline.ts");
pub const IMG_PARAMETERS_TS: &str = include_str!("../../../src/lib/types/imgParameters.ts");
pub const EXPORT_RENDERING_RS: &str = include_str!("../../src/export_rendering.rs");
pub const OPEN_IMAGE_TS: &str = include_str!("../../../src/lib/utils/openImage.ts");

// ── Reference colour maths ────────────────────────────────────────────────────
//
// Independent f64 reimplementations of the transfer functions the shaders use,
// so a test failure means the shader is wrong rather than that both copies
// drifted together.

pub fn srgb_to_linear(s: f64) -> f64 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(l: f64) -> f64 {
    if l <= 0.0031308 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Put an 8-bit value through linear space and back, applying `f` in between.
/// This is the reference the develop chain is measured against.
pub fn through_linear(v: u8, f: impl Fn(f64) -> f64) -> u8 {
    let linear = f(srgb_to_linear(v as f64 / 255.0));
    (linear_to_srgb(linear.clamp(0.0, 1.0)) * 255.0).round() as u8
}

// ── Synthetic charts ──────────────────────────────────────────────────────────

/// Expand an 8-bit sRGB value to u16 exactly: `v * 257` maps 0→0 and 255→65535
/// with no rounding, so an 8-bit round trip through the pipeline is exact.
pub fn u8_to_u16(v: u8) -> u16 {
    v as u16 * 257
}

/// u16 values chosen to sit on the interesting edges of the domain.
pub const EXTREME_VALUES: [u16; 8] = [0, 1, 2, 257, 32767, 32768, 65534, 65535];

pub fn solid(width: u32, height: u32, rgba: [u16; 4]) -> Rgba16Image {
    ImageBuffer::from_fn(width, height, |_, _| Rgba(rgba))
}

/// Horizontal neutral ramp spanning the full 8-bit range, R = G = B.
pub fn gray_ramp(width: u32, height: u32) -> Rgba16Image {
    ImageBuffer::from_fn(width, height, |x, _| {
        let v = ramp_value(x, width);
        Rgba([u8_to_u16(v), u8_to_u16(v), u8_to_u16(v), u16::MAX])
    })
}

/// Horizontal ramp with the three channels varying independently, so a channel
/// swap or a matrix transposition cannot hide.
pub fn rgb_ramp(width: u32, height: u32) -> Rgba16Image {
    ImageBuffer::from_fn(width, height, |x, _| {
        let v = ramp_value(x, width);
        Rgba([
            u8_to_u16(v),
            u8_to_u16(255 - v),
            u8_to_u16(v / 2),
            u16::MAX,
        ])
    })
}

/// Black, the three primaries, the three secondaries, white — one per column.
pub fn primaries() -> Rgba16Image {
    const PATCHES: [[u8; 3]; 8] = [
        [0, 0, 0],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [0, 255, 255],
        [255, 0, 255],
        [255, 255, 0],
        [255, 255, 255],
    ];
    ImageBuffer::from_fn(8, 1, |x, _| {
        let p = PATCHES[x as usize];
        Rgba([
            u8_to_u16(p[0]),
            u8_to_u16(p[1]),
            u8_to_u16(p[2]),
            u16::MAX,
        ])
    })
}

/// Every channel independently parked on a domain edge.
pub fn extremes_chart() -> Rgba16Image {
    let n = EXTREME_VALUES.len() as u32;
    ImageBuffer::from_fn(n, n, |x, y| {
        Rgba([
            EXTREME_VALUES[x as usize],
            EXTREME_VALUES[y as usize],
            EXTREME_VALUES[((x + y) % n) as usize],
            u16::MAX,
        ])
    })
}

/// Deterministic pseudo-random noise — non-degenerate input that is still
/// reproducible across runs and machines.
pub fn noise(width: u32, height: u32, seed: u32) -> Rgba16Image {
    let mut state = seed | 1;
    let mut next = move || {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    ImageBuffer::from_fn(width, height, |_, _| {
        Rgba([
            (next() >> 16) as u16,
            (next() >> 16) as u16,
            (next() >> 16) as u16,
            u16::MAX,
        ])
    })
}

/// Flatten a chart into the interleaved slice the renderer expects.
pub fn pixels(img: &Rgba16Image) -> Vec<u16> {
    img.as_raw().clone()
}

/// Value of the ramp at column `x`, spanning 0..=255 across `width` columns.
fn ramp_value(x: u32, width: u32) -> u8 {
    if width <= 1 {
        return 0;
    }
    (x * 255 / (width - 1)) as u8
}

/// A chart carrying both a neutral ramp (row 0) and a saturated ramp (row 1), so
/// a single render exercises the tone controls and the colour controls at once.
pub fn mixed_ramp(width: u32) -> Rgba16Image {
    ImageBuffer::from_fn(width, 2, |x, y| {
        let v = ramp_value(x, width);
        if y == 0 {
            Rgba([u8_to_u16(v), u8_to_u16(v), u8_to_u16(v), u16::MAX])
        } else {
            Rgba([
                u8_to_u16(v),
                u8_to_u16(255 - v),
                u8_to_u16(v / 2),
                u16::MAX,
            ])
        }
    })
}

// ── develop harness ───────────────────────────────────────────────────────────
//
// The export renderer is the only production entry point that runs the full
// develop chain and hands the result back to the CPU, so it doubles as the
// oracle for the shader chain the preview uses.

/// Render `img` with `sliders` at 8 bits and return the RGB bytes.
pub fn develop(img: &Rgba16Image, sliders: &SlidersPayload) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_image(img.as_raw(), w, h, 4, sliders, 8, |_| {}))
        .expect("render should succeed")
}

/// Render `img` with `sliders` at 16 bits, returning one u16 per channel.
pub fn develop16(img: &Rgba16Image, sliders: &SlidersPayload) -> Vec<u16> {
    let (w, h) = img.dimensions();
    let bytes = pollster::block_on(render_image(img.as_raw(), w, h, 4, sliders, 16, |_| {}))
        .expect("render should succeed");
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// The developed value of a solid 8-bit patch, as one RGB triple.
pub fn develop_patch(value: [u8; 3], sliders: &SlidersPayload) -> [u8; 3] {
    let img = solid(
        2,
        2,
        [
            u8_to_u16(value[0]),
            u8_to_u16(value[1]),
            u8_to_u16(value[2]),
            u16::MAX,
        ],
    );
    let out = develop(&img, sliders);
    [out[0], out[1], out[2]]
}

/// One developed patch per input value, all from a single render — a whole tone
/// curve for the cost of one GPU pass.
pub fn develop_patches(values: &[u8], sliders: &SlidersPayload) -> Vec<[u8; 3]> {
    let img: Rgba16Image = ImageBuffer::from_fn(values.len() as u32, 1, |x, _| {
        let v = u8_to_u16(values[x as usize]);
        Rgba([v, v, v, u16::MAX])
    });
    let out = develop(&img, sliders);
    (0..values.len())
        .map(|i| [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]])
        .collect()
}

// ── Shared GPU device ─────────────────────────────────────────────────────────

static DEVICE: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();

/// A process-wide wgpu device. Creating one per test dominates the runtime of
/// the GPU suite, and every test here is read-only with respect to the device.
pub fn device() -> &'static (wgpu::Device, wgpu::Queue) {
    DEVICE.get_or_init(|| {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .expect("no GPU adapter available — the GPU suite needs one");

            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("test_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                })
                .await
                .expect("GPU device creation failed")
        })
    })
}

/// Block until the GPU has drained, then read a buffer back to the CPU.
pub fn read_buffer(device: &wgpu::Device, staging: &wgpu::Buffer) -> Vec<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::MaintainBase::Wait);
    rx.recv().unwrap().expect("buffer map failed");

    let bytes = staging.slice(..).get_mapped_range().to_vec();
    staging.unmap();
    bytes
}

// ── calcParams harness ────────────────────────────────────────────────────────

/// The `Params` struct as 64 f32s, read straight back off the GPU.
///
/// Field offsets follow the WGSL layout documented in calcParams.wgsl; the
/// accessors below are the only place those offsets are hard-coded, and
/// `shader_contracts.rs` checks them against the shader source.
pub struct Params(pub Vec<f32>);

impl Params {
    /// Column-major 4×4 darkroom matrix. Normalises each channel to 0..1 between
    /// its black and white point; the output pair is applied separately, after
    /// gamma, via `out_black` and `out_scale`.
    pub fn dr_matrix(&self) -> &[f32] {
        &self.0[0..16]
    }
    pub fn gammas(&self) -> [f32; 3] {
        [self.0[16], self.0[17], self.0[18]]
    }
    /// Output black point, in linear light.
    pub fn out_black(&self) -> f32 {
        self.0[19]
    }
    /// Output white minus output black, in linear light.
    pub fn out_scale(&self) -> f32 {
        self.0[20]
    }
    /// White-balance matrix as three columns of three (the mat3x3 is padded to
    /// 16-byte column stride, so the fourth lane of each column is skipped).
    pub fn wb_matrix(&self) -> [[f32; 3]; 3] {
        [
            [self.0[24], self.0[25], self.0[26]],
            [self.0[28], self.0[29], self.0[30]],
            [self.0[32], self.0[33], self.0[34]],
        ]
    }
    pub fn exposure(&self) -> f32 {
        self.0[36]
    }
    pub fn contrast(&self) -> f32 {
        self.0[37]
    }
    pub fn brightness(&self) -> f32 {
        self.0[38]
    }
    pub fn highlights(&self) -> f32 {
        self.0[39]
    }
    pub fn shadows(&self) -> f32 {
        self.0[40]
    }
    pub fn whites(&self) -> f32 {
        self.0[41]
    }
    pub fn blacks(&self) -> f32 {
        self.0[42]
    }
    pub fn hue_sat_matrix(&self) -> &[f32] {
        &self.0[44..60]
    }
    pub fn vibrance(&self) -> f32 {
        self.0[60]
    }

    /// Every meaningful lane, skipping the explicit padding slots.
    pub fn significant_values(&self) -> Vec<(usize, f32)> {
        const PADDING: [usize; 10] = [21, 22, 23, 27, 31, 35, 43, 61, 62, 63];
        self.0
            .iter()
            .enumerate()
            .filter(|(i, _)| !PADDING.contains(i))
            .map(|(i, v)| (i, *v))
            .collect()
    }
}

/// Build a params storage buffer by running calcParams.wgsl for `sliders`.
fn calc_params_buffer(sliders: &SlidersPayload, extra_usage: wgpu::BufferUsages) -> wgpu::Buffer {
    calc_buffers(sliders, &ViewPayload::default(), extra_usage).0
}

/// Run calcParams.wgsl for `sliders` at `view`, returning both the `Params` and
/// the `SharpParams` buffers it writes.
fn calc_buffers(
    sliders: &SlidersPayload,
    view: &ViewPayload,
    extra_usage: wgpu::BufferUsages,
) -> (wgpu::Buffer, wgpu::Buffer) {
    let (device, queue) = device();

    let slider_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test_slider_buf"),
        contents: &sliders_to_bytes(sliders),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test_view_buf"),
        contents: &view_to_bytes(view),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_params_buf"),
        size: PARAMS_BYTES,
        usage: wgpu::BufferUsages::STORAGE | extra_usage,
        mapped_at_creation: false,
    });

    let sharp_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_sharp_buf"),
        size: SHARP_PARAMS_BYTES,
        usage: wgpu::BufferUsages::STORAGE | extra_usage,
        mapped_at_creation: false,
    });

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("calcParams"),
        source: wgpu::ShaderSource::Wgsl(
            open_darkroom_lib::export_rendering::CALC_PARAMS_WGSL.into(),
        ),
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("test_calc_pipeline"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("test_calc_bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: slider_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: view_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: sharp_buf.as_entire_binding(),
            },
        ],
    });

    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    queue.submit(std::iter::once(enc.finish()));
    let _ = device.poll(wgpu::MaintainBase::Wait);

    (params_buf, sharp_buf)
}

/// The `SharpParams` struct calcParams.wgsl writes, read back as f32 lanes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharpParams {
    pub usm_amount: f32,
    pub usm_sigma: f32,
    pub usm_luma_threshold: f32,
    pub usm_detail_threshold: f32,
    pub texture_amount: f32,
    pub texture_sigma_lo: f32,
    pub texture_sigma_hi: f32,
    pub clarity_amount: f32,
    pub clarity_sigma: f32,
}

/// Run calcParams.wgsl for a view and read the `SharpParams` back. The
/// dimensions are the whole image's — clarity's radius is a fraction of them.
pub fn run_calc_sharp(
    sliders: &SlidersPayload,
    render_scale: f32,
    full_width: u32,
    full_height: u32,
) -> SharpParams {
    let (device, queue) = device();

    let (_, sharp_buf) = calc_buffers(
        sliders,
        &ViewPayload {
            render_scale,
            full_width: full_width as f32,
            full_height: full_height as f32,
        },
        wgpu::BufferUsages::COPY_SRC,
    );

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_sharp_staging"),
        size: SHARP_PARAMS_BYTES,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_buffer_to_buffer(&sharp_buf, 0, &staging, 0, SHARP_PARAMS_BYTES);
    queue.submit(std::iter::once(enc.finish()));

    let bytes = read_buffer(device, &staging);
    let lane = |i: usize| f32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);

    SharpParams {
        usm_amount: lane(0),
        usm_sigma: lane(1),
        usm_luma_threshold: lane(2),
        usm_detail_threshold: lane(3),
        texture_amount: lane(4),
        texture_sigma_lo: lane(5),
        texture_sigma_hi: lane(6),
        clarity_amount: lane(7),
        clarity_sigma: lane(8),
    }
}

/// Run calcParams.wgsl and read the resulting `Params` struct back.
pub fn run_calc_params(sliders: &SlidersPayload) -> Params {
    let (device, queue) = device();

    let params_buf = calc_params_buffer(sliders, wgpu::BufferUsages::COPY_SRC);

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_params_staging"),
        size: PARAMS_BYTES,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_buffer_to_buffer(&params_buf, 0, &staging, 0, PARAMS_BYTES);
    queue.submit(std::iter::once(enc.finish()));

    let bytes = read_buffer(device, &staging);
    Params(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

// ── histogram.wgsl harness ────────────────────────────────────────────────────

/// Total weight each pixel contributes, mirroring `VOTE` in histogram.wgsl.
pub const HIST_VOTE: u32 = 256;

/// Run histogram.wgsl over `img` with `sliders` applied, returning the R, G and
/// B bin arrays. This is the shader the preview's histogram panel uses.
pub fn run_histogram(img: &Rgba16Image, sliders: &SlidersPayload) -> [[u32; HIST_BINS]; 3] {
    let (device, queue) = device();
    let (width, height) = img.dimensions();

    let params_buf = calc_params_buffer(sliders, wgpu::BufferUsages::empty());

    // Upload the image exactly the way the preview path does.
    let lut = build_srgb_to_linear_lut_u16();
    let linear = linearize_u16(img.as_raw(), 4, &lut);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test_hist_tex"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &linear,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 8),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    // One buffer holding the three channels end to end, matching histogram.wgsl.
    let bin_bytes = (HIST_BINS * 3 * 4) as u64;
    let bins = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("bins"),
        contents: &vec![0u8; bin_bytes as usize],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("histogram"),
        source: wgpu::ShaderSource::Wgsl(HISTOGRAM_WGSL.into()),
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("test_hist_pipeline"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let view = texture.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("test_hist_bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: bins.as_entire_binding(),
            },
        ],
    });

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_hist_staging"),
        size: bin_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
    }
    enc.copy_buffer_to_buffer(&bins, 0, &staging, 0, bin_bytes);
    queue.submit(std::iter::once(enc.finish()));

    let bytes = read_buffer(device, &staging);
    let mut out = [[0u32; HIST_BINS]; 3];
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        out[i / HIST_BINS][i % HIST_BINS] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    out
}

// ── Assertions ────────────────────────────────────────────────────────────────

/// Mean bin index of a histogram, weighting each bin by its count.
pub fn weighted_mean_bin(bins: &[u32; HIST_BINS]) -> f64 {
    let total: f64 = bins.iter().map(|&c| c as f64).sum();
    if total == 0.0 {
        return 0.0;
    }
    bins.iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum::<f64>()
        / total
}

/// Assert two 8-bit buffers agree within `tolerance`, reporting the worst pixel.
pub fn assert_close_u8(actual: &[u8], expected: &[u8], tolerance: i32, context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{context}: length mismatch ({} vs {})",
        actual.len(),
        expected.len()
    );

    let mut worst = (0usize, 0i32);
    for (i, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
        let delta = (a as i32 - e as i32).abs();
        if delta > worst.1 {
            worst = (i, delta);
        }
    }

    assert!(
        worst.1 <= tolerance,
        "{context}: worst delta {} at byte {} (got {}, expected {}), tolerance {}",
        worst.1,
        worst.0,
        actual[worst.0],
        expected[worst.0],
        tolerance
    );
}
