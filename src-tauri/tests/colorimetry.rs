// src-tauri/tests/colorimetry.rs
//
// Layer 2: the colorimetric constants and the Planckian locus in
// calcParams.wgsl, measured against published values rather than against our
// own restatement of them.
//
// The rest of the white-balance suite in `sliders_gpu.rs` is directional —
// warmer looks warmer, the R/B ratio rises, luminance holds. That passes for a
// transposed adaptation matrix or a mistyped locus coefficient as long as the
// ordering survives. These tests pin the numbers themselves: the sRGB and
// Bradford matrices against their standard definitions, and the locus against
// tabulated blackbody chromaticities.
//
// Everything runs on the GPU through a probe entry point appended to the
// shipped shader source, so what is measured is the constant the pipeline
// actually compiles, not a copy of it kept in the test.

mod common;

use common::{device, read_buffer};

use open_darkroom_lib::export_rendering::{CALC_PARAMS_WGSL, PARAMS_BYTES, SLIDERS_BYTES};
use wgpu::util::DeviceExt;

// ── Published reference values ────────────────────────────────────────────────

/// CIE 1931 XYZ of the D65 white point, the illuminant sRGB is defined against.
const D65_XYZ: [f64; 3] = [0.95047, 1.00000, 1.08883];

/// ITU-R BT.709 luminance coefficients.
const BT709: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// Tabulated chromaticities of a Planckian (blackbody) radiator under the CIE
/// 1931 2° observer. 2856 K is CIE standard illuminant A.
///
/// These are the locus itself, not the daylight locus: D65's (0.3127, 0.3290)
/// sits slightly above the blackbody curve and is deliberately not used here.
const PLANCKIAN_LOCUS: [(f32, f64, f64); 15] = [
    (2000.0, 0.5267, 0.4133),
    (2500.0, 0.4770, 0.4137),
    (2856.0, 0.44758, 0.40745),
    (3000.0, 0.4369, 0.4041),
    (3500.0, 0.4053, 0.3907),
    (4000.0, 0.3805, 0.3768),
    (4500.0, 0.3608, 0.3636),
    (5000.0, 0.3451, 0.3516),
    (5500.0, 0.3325, 0.3411),
    (6000.0, 0.3221, 0.3318),
    (6500.0, 0.3135, 0.3237),
    (7000.0, 0.3064, 0.3166),
    (8000.0, 0.2952, 0.3048),
    (9000.0, 0.2869, 0.2956),
    (10000.0, 0.2807, 0.2884),
];

/// The temperature range the UI actually offers.
const UI_TEMP_RANGE: (f32, f32) = (2200.0, 8800.0);

/// Chromaticity slack against the table above.
///
/// The cubic fit the shader uses tracks the locus to about 1.6e-4 from 4000 K
/// up and loses roughly half a thousandth toward the bottom of its range, so
/// the bound is split rather than set to the worse of the two everywhere —
/// most of the slider deserves the tighter one.
fn locus_tol(temp: f32) -> f64 {
    if temp < 4000.0 {
        8e-4
    } else {
        3e-4
    }
}

/// Slack for a 3×3 product that should be the identity, evaluated in f32.
const MATRIX_TOL: f32 = 1e-5;

// ── Probe harness ─────────────────────────────────────────────────────────────

/// Extra entry points compiled alongside the shipped shader, reading out
/// constants and functions that production code only uses internally.
const PROBE_WGSL: &str = r#"

@group(0) @binding(2) var<storage, read_write> probe_out: array<vec4f>;
@group(0) @binding(3) var<storage, read> probe_temps: array<f32>;

@compute @workgroup_size(1)
fn probe_locus(@builtin(global_invocation_id) gid: vec3u) {
    probe_out[gid.x] = vec4f(cct_to_xyz(probe_temps[gid.x]), 0.0);
}

@compute @workgroup_size(1)
fn probe_matrices() {
    let bradford = M_BRADFORD_INV * M_BRADFORD;
    let xyz      = M_XYZ_TO_RGB * M_RGB_TO_XYZ;
    for (var c = 0u; c < 3u; c++) {
        probe_out[c]      = vec4f(bradford[c], 0.0);
        probe_out[3u + c] = vec4f(xyz[c],      0.0);
    }
    probe_out[6] = vec4f(M_RGB_TO_XYZ * vec3f(1.0, 1.0, 1.0), 0.0);
    probe_out[7] = vec4f(M_RGB_TO_XYZ[0].y, M_RGB_TO_XYZ[1].y, M_RGB_TO_XYZ[2].y, 0.0);
    probe_out[8] = vec4f(LR, LG, LB, 0.0);
}
"#;

/// Run one probe entry point and read back `count` vec4 slots.
fn run_probe(entry: &str, temps: &[f32], count: usize) -> Vec<[f32; 4]> {
    let (device, queue) = device();

    let source = format!("{CALC_PARAMS_WGSL}{PROBE_WGSL}");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("calcParams+probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    // The probes ignore bindings 0 and 1, but declaring and binding all four
    // keeps the layout valid whichever entry point is selected.
    let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..4)
        .map(|i| wgpu::BindGroupLayoutEntry {
            binding: i,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: match i {
                    0 => wgpu::BufferBindingType::Uniform,
                    3 => wgpu::BufferBindingType::Storage { read_only: true },
                    _ => wgpu::BufferBindingType::Storage { read_only: false },
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        })
        .collect();

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("probe_bgl"),
        entries: &entries,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("probe_pl"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("probe_pipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some(entry),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let sliders = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe_sliders"),
        size: SLIDERS_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe_params"),
        size: PARAMS_BYTES,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let out_bytes = (count.max(1) * 16) as u64;
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe_out"),
        size: out_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // A storage array needs at least one element even when the entry point
    // being run never reads it.
    let temp_data: Vec<f32> = if temps.is_empty() {
        vec![0.0]
    } else {
        temps.to_vec()
    };
    let temp_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("probe_temps"),
        contents: bytemuck::cast_slice(&temp_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("probe_bg"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: sliders.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: temp_buf.as_entire_binding(),
            },
        ],
    });

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe_staging"),
        size: out_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(temp_data.len() as u32, 1, 1);
    }
    enc.copy_buffer_to_buffer(&out, 0, &staging, 0, out_bytes);
    queue.submit(std::iter::once(enc.finish()));

    read_buffer(device, &staging)
        .chunks_exact(16)
        .map(|c| {
            let mut v = [0f32; 4];
            for (i, f) in v.iter_mut().enumerate() {
                *f = f32::from_le_bytes([c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
            }
            v
        })
        .collect()
}

/// CIE xy chromaticity of an XYZ triple.
fn chromaticity(xyz: [f32; 4]) -> (f64, f64) {
    let (x, y, z) = (xyz[0] as f64, xyz[1] as f64, xyz[2] as f64);
    let sum = x + y + z;
    (x / sum, y / sum)
}

/// The locus chromaticities the shader produces for `temps`.
fn locus(temps: &[f32]) -> Vec<(f64, f64)> {
    run_probe("probe_locus", temps, temps.len())
        .into_iter()
        .map(chromaticity)
        .collect()
}

// ── Matrix constants ──────────────────────────────────────────────────────────

fn assert_identity(label: &str, columns: &[[f32; 4]]) {
    for (col, v) in columns.iter().enumerate() {
        for row in 0..3 {
            let want = if col == row { 1.0 } else { 0.0 };
            assert!(
                (v[row] - want).abs() < MATRIX_TOL,
                "{label}[{col}][{row}]: expected {want}, got {} — the pair is not a true inverse",
                v[row]
            );
        }
    }
}

/// `M_BRADFORD_INV` is written out as literals rather than inverted at runtime,
/// so nothing but this checks that it is the inverse of `M_BRADFORD`. A wrong
/// digit here tints every temperature except the reference one, which is
/// exactly where the directional white-balance tests are blind.
#[test]
fn the_bradford_matrices_are_a_true_inverse_pair() {
    let out = run_probe("probe_matrices", &[], 9);
    assert_identity("M_BRADFORD_INV · M_BRADFORD", &out[0..3]);
}

#[test]
fn the_xyz_and_rgb_matrices_are_a_true_inverse_pair() {
    let out = run_probe("probe_matrices", &[], 9);
    assert_identity("M_XYZ_TO_RGB · M_RGB_TO_XYZ", &out[3..6]);
}

/// sRGB is defined against D65: its primaries must sum to that white point.
/// This is what identifies `M_RGB_TO_XYZ` as the sRGB matrix rather than some
/// other RGB space's, which the round-trip test above cannot tell apart.
#[test]
fn the_rgb_to_xyz_matrix_maps_white_to_d65() {
    let white = run_probe("probe_matrices", &[], 9)[6];

    for (i, (&got, &want)) in white.iter().zip(D65_XYZ.iter()).enumerate() {
        assert!(
            (got as f64 - want).abs() < 1e-5,
            "white point component {i}: expected {want}, got {got}"
        );
    }
}

/// The Y row of the sRGB matrix *is* the BT.709 luminance vector, and the
/// shader also keeps it as three separate constants for the saturation and
/// white-balance maths. Both must agree with the standard, and with each other.
#[test]
fn the_luminance_coefficients_are_bt709() {
    let out = run_probe("probe_matrices", &[], 9);
    let (y_row, constants) = (out[7], out[8]);

    for i in 0..3 {
        // The matrix carries seven decimals where BT.709 publishes four.
        assert!(
            (y_row[i] as f64 - BT709[i]).abs() < 2e-4,
            "M_RGB_TO_XYZ Y row component {i}: expected ~{}, got {}",
            BT709[i],
            y_row[i]
        );
        assert!(
            (constants[i] as f64 - BT709[i]).abs() < 1e-6,
            "LR/LG/LB component {i}: expected {}, got {}",
            BT709[i],
            constants[i]
        );
    }
}

// ── Planckian locus ───────────────────────────────────────────────────────────

/// The temperature slider is only meaningful if the illuminant it names is the
/// one it builds. Every other white-balance test asks whether the image moved
/// in the right direction; this asks whether it moved to the right place.
#[test]
fn the_locus_matches_published_blackbody_chromaticities() {
    let temps: Vec<f32> = PLANCKIAN_LOCUS.iter().map(|&(t, _, _)| t).collect();
    let got = locus(&temps);

    for (&(temp, want_x, want_y), &(x, y)) in PLANCKIAN_LOCUS.iter().zip(got.iter()) {
        let tol = locus_tol(temp);
        assert!(
            (x - want_x).abs() < tol,
            "{temp} K: chromaticity x expected {want_x:.5}, got {x:.5} (off by {:.1e})",
            (x - want_x).abs()
        );
        assert!(
            (y - want_y).abs() < tol,
            "{temp} K: chromaticity y expected {want_y:.5}, got {y:.5} (off by {:.1e})",
            (y - want_y).abs()
        );
    }
}

/// The locus is a smooth curve, so the piecewise fit must not step at the
/// temperatures where it changes branch. A step here is a visible jump in
/// colour from one slider position to the next.
#[test]
fn the_locus_is_continuous_across_its_branch_boundaries() {
    // 2e-4 is well inside the fit's own accuracy and an order of magnitude
    // below a step a photographer would see.
    const STEP_TOL: f64 = 2e-4;

    for boundary in [2222.0f32, 4000.0] {
        let got = locus(&[boundary - 0.1, boundary + 0.1]);
        let (below, above) = (got[0], got[1]);

        assert!(
            (below.0 - above.0).abs() < STEP_TOL && (below.1 - above.1).abs() < STEP_TOL,
            "the locus steps at {boundary} K: ({:.6}, {:.6}) → ({:.6}, {:.6})",
            below.0,
            below.1,
            above.0,
            above.1
        );
    }
}

/// Rising temperature walks the locus steadily across the range the slider
/// exposes. `the_red_to_blue_ratio_rises_with_temperature` checks the same idea
/// through a developed image at five points; this checks the curve itself,
/// finely enough to catch a local reversal between them.
///
/// Scoped to the UI's range rather than the shader's clamp: `cct_to_xyz` clamps
/// at 1000 K, below the 1667 K floor the fit is published for, and the curve
/// does double back down there.
#[test]
fn the_locus_runs_monotonically_across_the_slider_range() {
    let (min, max) = UI_TEMP_RANGE;
    let temps: Vec<f32> = (0..=132)
        .map(|i| min + (max - min) * i as f32 / 132.0)
        .collect();
    let got = locus(&temps);

    for i in 1..got.len() {
        assert!(
            got[i].0 < got[i - 1].0,
            "chromaticity x rose from {:.0} K to {:.0} K ({:.6} → {:.6})",
            temps[i - 1],
            temps[i],
            got[i - 1].0,
            got[i].0
        );
    }

    // y is not monotone over the whole slider: the locus reaches its highest
    // point near 2250 K and falls away on both sides, so the check starts past
    // the bend. x carries the ordering below it.
    let bend = temps.iter().position(|&t| t >= 2500.0).unwrap();
    for i in bend + 1..got.len() {
        assert!(
            got[i].1 < got[i - 1].1,
            "chromaticity y rose from {:.0} K to {:.0} K ({:.6} → {:.6})",
            temps[i - 1],
            temps[i],
            got[i - 1].1,
            got[i].1
        );
    }
}
