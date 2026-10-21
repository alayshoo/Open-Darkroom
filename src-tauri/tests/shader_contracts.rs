// src-tauri/tests/shader_contracts.rs
//
// Layer 1: structural drift guards. No GPU, no image data — these read the
// source files as text and assert that the same struct, declared in WGSL, Rust
// and TypeScript, still describes the same bytes in the same order.
//
// Nothing here checks that the pipeline produces a *good* image; these catch
// the class of bug where it silently produces a wrong one because a field was
// inserted in one language and not the others.

mod common;

use common::{
    CALC_PARAMS_PIPELINE_TS, CHROMATICITY_DRAW_WGSL, COLOR_SPACE_SCALE_WGSL, COLOR_SPACE_WGSL,
    COLOR_VOLUME_DRAW_WGSL, EXPORT_RENDERING_RS, EXPORT_SETTINGS_TS, HISTOGRAM_WGSL, OPEN_IMAGE_TS,
    OUTPUT_SPACE_BUFFER_TS, OUTPUT_SPACE_TS, WORKING_SPACE_TS,
};

use open_darkroom_lib::export_rendering::{
    CALC_PARAMS_WGSL, CALC_SHARP_TEXTURE_WGSL, COLOR_SPACE_ENCODE_WGSL, COMPOSITE_WGSL,
    CURVES_OFFSET, CURVE_LUT_ENTRIES, DEVELOP_WGSL, PARAMS_BYTES, SHARP_PARAMS_BYTES, SLIDERS_BYTES, SLIDER_COUNT,
    SOURCE_SPACE_BYTES, TONE_MAP_BYTES, VIEW_BYTES,
};
use open_darkroom_lib::export_rendering::OUTPUT_SPACE_BYTES;
use open_darkroom_lib::crop::CROP_VIEW_BYTES;
use open_darkroom_lib::icc::SRGB_TO_WORKING;
use open_darkroom_lib::output_space::{OutputSpace, Transfer};
use open_darkroom_lib::image_opening::{OPEN_CANCELLED, PAYLOAD_HEADER_BYTES};

/// The frontend has to tell a dismissed file picker apart from an open that
/// failed after the loaded image was released — the first leaves the current
/// image alone, the second means it is already gone. That distinction rides on
/// one string matching across the two languages, and nothing else would catch it
/// drifting: the mismatch turns a cancelled open into a cleared canvas.
#[test]
fn typescript_matches_the_cancelled_open_sentinel() {
    let quoted = format!("\"{OPEN_CANCELLED}\"");
    assert!(
        OPEN_IMAGE_TS.contains(&format!("OPEN_CANCELLED = {quoted}")),
        "openImage.ts does not declare OPEN_CANCELLED = {quoted}"
    );
}

/// The in-browser suite keeps its own copy of the untagged-file matrix so it can
/// drive the chain the way the Rust suite does. A copy that drifts would leave
/// those tests asserting a conversion the app no longer performs — and they are
/// the only tests that check the TypeScript wiring at all.
#[test]
fn the_browser_suite_uses_the_same_untagged_matrix_as_rust() {
    let numbers: Vec<f32> = WORKING_SPACE_TS
        .split("SRGB_TO_WORKING = new Float32Array([")
        .nth(1)
        .expect("workingSpace.ts does not declare SRGB_TO_WORKING")
        .split("])")
        .next()
        .unwrap()
        .split(',')
        .filter_map(|t| t.trim().parse().ok())
        .collect();

    assert_eq!(numbers.len(), 9, "expected nine elements, got {numbers:?}");
    for (i, got) in numbers.iter().enumerate() {
        assert!(
            (got - SRGB_TO_WORKING[i]).abs() < 1e-7,
            "workingSpace.ts element {i}: expected {}, got {got}",
            SRGB_TO_WORKING[i]
        );
    }
}

/// The frontend reads the payload at a fixed offset, so the header size is a
/// number two languages have to agree on. They have disagreed before: the
/// pixels then come out of the middle of the header and nothing says why.
#[test]
fn typescript_reads_the_payload_header_at_the_same_size_as_rust() {
    let declared = format!("HEADER_BYTES = {PAYLOAD_HEADER_BYTES};");
    assert!(
        OPEN_IMAGE_TS.contains(&declared),
        "openImage.ts does not declare {declared}"
    );
}

/// Every shader flattened by build.rs — the export path's, and the frontend's
/// colour-space tools, which run the same develop chain.
const SHADERS: [(&str, &str); 10] = [
    ("calcParams.wgsl", CALC_PARAMS_WGSL),
    ("develop.wgsl", DEVELOP_WGSL),
    ("calcSharpTexture.wgsl", CALC_SHARP_TEXTURE_WGSL),
    ("composite.wgsl", COMPOSITE_WGSL),
    ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
    ("histogram.wgsl", HISTOGRAM_WGSL),
    ("colorSpace.wgsl", COLOR_SPACE_WGSL),
    ("colorSpaceScale.wgsl", COLOR_SPACE_SCALE_WGSL),
    ("chromaticityDraw.wgsl", CHROMATICITY_DRAW_WGSL),
    ("colorVolumeDraw.wgsl", COLOR_VOLUME_DRAW_WGSL),
];

/// The shaders that run the adjustment chain, and so have to carry exactly the
/// same copy of it.
const CHAIN_SHADERS: [(&str, &str); 3] = [
    ("develop.wgsl", DEVELOP_WGSL),
    ("histogram.wgsl", HISTOGRAM_WGSL),
    ("colorSpace.wgsl", COLOR_SPACE_WGSL),
];

// ── WGSL parsing ──────────────────────────────────────────────────────────────

/// A single field of a WGSL struct.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Field {
    name: String,
    ty: String,
}

/// Extract the fields of `struct <name> { … }` from WGSL source, in order.
fn wgsl_struct(source: &str, name: &str) -> Vec<Field> {
    let header = format!("struct {name} {{");
    let start = source
        .find(&header)
        .unwrap_or_else(|| panic!("struct {name} not found"))
        + header.len();
    let len = source[start..]
        .find('}')
        .unwrap_or_else(|| panic!("struct {name} is not closed"));

    source[start..start + len]
        .lines()
        .filter_map(|line| {
            // Drop comments, then accept `name: type,`
            let code = line.split("//").next().unwrap().trim().trim_end_matches(',');
            let (name, ty) = code.split_once(':')?;
            let (name, ty) = (name.trim(), ty.trim());
            if name.is_empty() || ty.is_empty() {
                return None;
            }
            Some(Field {
                name: name.to_string(),
                ty: ty.to_string(),
            })
        })
        .collect()
}

/// Size in bytes of a WGSL type as laid out in a uniform/storage buffer.
fn wgsl_size(ty: &str) -> usize {
    match ty {
        "f32" | "u32" | "i32" => 4,
        "vec2f" => 8,
        "vec3f" | "vec4f" => 16,
        // A mat3x3 has 16-byte column stride, so it occupies 48 bytes, not 36.
        "mat3x3<f32>" => 48,
        "mat4x4<f32>" => 64,
        array if array.starts_with("array<") => {
            let (elem, count) = array["array<".len()..array.len() - 1]
                .rsplit_once(',')
                .unwrap_or_else(|| panic!("unsized array in layout calculation: {array}"));
            wgsl_size(elem.trim()) * count.trim().parse::<usize>().expect("array length")
        }
        other => panic!("unhandled WGSL type in layout calculation: {other}"),
    }
}

fn struct_size(fields: &[Field]) -> usize {
    fields.iter().map(|f| wgsl_size(&f.ty)).sum()
}

// ── Field-order extraction from Rust and TypeScript ───────────────────────────

/// Field names referenced through `s.` inside a delimited block, in order.
///
/// Both `sliders_to_bytes` (Rust) and `slidersToArray` (TypeScript) are flat
/// lists of `s.field` expressions, so the same extraction works for both.
fn accessed_fields(source: &str, after: &str, open: char, close: char) -> Vec<String> {
    let anchor = source
        .find(after)
        .unwrap_or_else(|| panic!("anchor {after:?} not found"));
    // Search past the anchor itself — `let values: [f32; N]` contains a `[`.
    let anchor_end = anchor + after.len();
    let start = anchor_end
        + source[anchor_end..]
            .find(open)
            .unwrap_or_else(|| panic!("no {open:?} after {after:?}"));
    let len = source[start..]
        .find(close)
        .unwrap_or_else(|| panic!("no {close:?} after {after:?}"));
    let block = &source[start..start + len];

    let mut names = Vec::new();
    let mut rest = block;
    while let Some(at) = rest.find("s.") {
        rest = &rest[at + 2..];
        let end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(end);
        if !name.is_empty() {
            names.push(name.to_string());
        }
        rest = tail;
    }
    names
}

fn camel_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── Sliders: WGSL ↔ Rust ↔ TypeScript ─────────────────────────────────────────

/// Snake-case names of the per-colour HSL sliders, in the order `HSL_KEYS`
/// lists them in imgParameters.ts. The TypeScript packer and defaults expand
/// that list rather than naming each field.
fn ts_hsl_fields() -> Vec<String> {
    let quoted = |anchor: &str| -> Vec<String> {
        let at = common::IMG_PARAMETERS_TS
            .find(anchor)
            .unwrap_or_else(|| panic!("{anchor:?} not found in imgParameters.ts"))
            + anchor.len();
        let end = common::IMG_PARAMETERS_TS[at..].find(']').expect("unterminated list");
        common::IMG_PARAMETERS_TS[at..at + end]
            .split(',')
            .map(|t| t.trim().trim_matches('"').to_string())
            .filter(|t| !t.is_empty())
            .collect()
    };
    let bands = quoted("HSL_BANDS = [");
    let channels = quoted("(band) =>\n    ([");
    assert_eq!((bands.len(), channels.len()), (8, 3), "{bands:?} {channels:?}");
    bands
        .iter()
        .flat_map(|band| {
            channels
                .iter()
                .map(move |channel| format!("hsl_{band}_{}", channel.to_lowercase()))
        })
        .collect()
}

#[test]
fn sliders_struct_is_all_f32_and_matches_the_declared_size() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "Sliders");

    assert_eq!(
        fields.len(),
        SLIDER_COUNT,
        "unexpected slider count: {fields:#?}"
    );
    assert!(
        fields.iter().all(|f| f.ty == "f32"),
        "every slider must be an f32 — the CPU packers write a flat f32 array"
    );

    // A struct in the uniform address space is 16-aligned, so its size rounds up
    // to a multiple of 16 and the buffer has to be allocated at that size even
    // though the packers only ever write the fields.
    let padded = struct_size(&fields).next_multiple_of(16);
    assert_eq!(
        padded, SLIDERS_BYTES,
        "SLIDERS_BYTES disagrees with the WGSL Sliders struct"
    );
}

#[test]
fn rust_slider_packing_matches_the_wgsl_struct() {
    let wgsl: Vec<String> = wgsl_struct(CALC_PARAMS_WGSL, "Sliders")
        .into_iter()
        .map(|f| f.name)
        .collect();

    let rust = accessed_fields(EXPORT_RENDERING_RS, "let values: [f32; SLIDER_COUNT]", '[', ']');

    assert_eq!(
        rust, wgsl,
        "sliders_to_bytes writes fields in a different order than calcParams.wgsl reads them"
    );
}

#[test]
fn typescript_slider_packing_matches_the_wgsl_struct() {
    let wgsl: Vec<String> = wgsl_struct(CALC_PARAMS_WGSL, "Sliders")
        .into_iter()
        .map(|f| f.name)
        .collect();

    let mut ts: Vec<String> = accessed_fields(CALC_PARAMS_PIPELINE_TS, "function slidersToArray", '[', ']')
        .iter()
        .map(|n| camel_to_snake(n))
        .collect();
    assert!(
        CALC_PARAMS_PIPELINE_TS.contains("...HSL_KEYS.map((key) => s[key]),\n    ]);"),
        "slidersToArray must end by packing HSL_KEYS"
    );
    ts.extend(ts_hsl_fields());

    assert_eq!(
        ts, wgsl,
        "slidersToArray packs fields in a different order than calcParams.wgsl reads them"
    );
}

#[test]
fn typescript_declares_the_same_buffer_sizes_as_rust() {
    // The constants are aligned on their `=`, so the spacing around it varies.
    let number_after = |name: &str| -> usize {
        let at = CALC_PARAMS_PIPELINE_TS
            .find(name)
            .unwrap_or_else(|| panic!("{name} not found in calcParamsPipeline.ts"));
        let rest = &CALC_PARAMS_PIPELINE_TS[at + name.len()..];
        let eq = rest
            .find('=')
            .unwrap_or_else(|| panic!("{name} is not an assignment"));
        rest[eq + 1..]
            .split(';')
            .next()
            .unwrap()
            .trim()
            .parse()
            .expect("expected a numeric literal")
    };

    assert_eq!(
        number_after("SLIDERS_BUFFER_SIZE"),
        SLIDERS_BYTES,
        "SLIDERS_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
    assert_eq!(
        number_after("PARAMS_BUFFER_SIZE"),
        PARAMS_BYTES as usize,
        "PARAMS_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
    assert_eq!(
        number_after("SOURCE_BUFFER_SIZE"),
        SOURCE_SPACE_BYTES as usize,
        "SOURCE_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
    assert_eq!(
        number_after("VIEW_BUFFER_SIZE"),
        VIEW_BYTES,
        "VIEW_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
    assert_eq!(
        number_after("TONE_MAP_BUFFER_SIZE"),
        TONE_MAP_BYTES,
        "TONE_MAP_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
}

#[test]
fn tone_map_struct_is_all_f32_and_matches_the_declared_size() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "ToneMap");

    assert!(
        fields.iter().all(|f| f.ty == "f32"),
        "every ToneMap field must be an f32 — the CPU packers write a flat f32 array"
    );
    assert_eq!(
        struct_size(&fields).next_multiple_of(16),
        TONE_MAP_BYTES,
        "TONE_MAP_BYTES disagrees with the WGSL ToneMap struct"
    );
}

/// The view is bound to calcParams as a uniform, so it carries the same 16-byte
/// alignment rule the sliders do.
#[test]
fn view_struct_is_all_f32_and_matches_the_declared_size() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "View");

    assert!(
        fields.iter().all(|f| f.ty == "f32"),
        "every View field must be an f32 — the CPU packers write a flat f32 array"
    );
    assert_eq!(
        struct_size(&fields).next_multiple_of(16),
        VIEW_BYTES,
        "VIEW_BYTES disagrees with the WGSL View struct"
    );
    assert_eq!(
        fields.first().map(|f| f.name.as_str()),
        Some("render_scale"),
        "render_scale must be the first field — it is the only one the packers write"
    );
}

/// The crop uniform is packed as eight flat f32 on both sides, and the encode
/// stage and the histogram both read it.
#[test]
fn crop_view_struct_matches_the_declared_size_everywhere() {
    for (name, source) in [
        ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
        ("histogram.wgsl", HISTOGRAM_WGSL),
    ] {
        let fields = wgsl_struct(source, "CropView");
        assert!(
            fields.iter().all(|f| f.ty == "vec2f"),
            "{name}: every CropView field must be a vec2f"
        );
        assert_eq!(
            struct_size(&fields).next_multiple_of(16),
            CROP_VIEW_BYTES,
            "{name}: CROP_VIEW_BYTES disagrees with the WGSL CropView struct"
        );
    }
    assert!(
        common::CROP_TS.contains(&format!("CROP_VIEW_BUFFER_SIZE = {CROP_VIEW_BYTES};")),
        "crop.ts does not declare CROP_VIEW_BUFFER_SIZE = {CROP_VIEW_BYTES}"
    );
}

/// `SlidersPayload::default()` exists so the tests can express "neutral". It is
/// only meaningful if it agrees with the defaults the UI actually starts from.
#[test]
fn rust_default_sliders_match_the_frontend_defaults() {
    let block = {
        let anchor = "defaultSlidersRGB: Sliders = {";
        let at = common::IMG_PARAMETERS_TS
            .find(anchor)
            .expect("defaultSlidersRGB not found") + anchor.len();
        let end = common::IMG_PARAMETERS_TS[at..].find('}').expect("unterminated");
        &common::IMG_PARAMETERS_TS[at..at + end]
    };

    let mut from_ts = std::collections::HashMap::new();
    for line in block.lines() {
        let code = line.split("//").next().unwrap().trim().trim_end_matches(',');
        let Some((name, value)) = code.split_once(':') else { continue };
        let value = match value.trim() {
            "true" => 1.0,
            "false" => 0.0,
            n => match n.parse::<f32>() {
                Ok(v) => v,
                Err(_) => continue,
            },
        };
        from_ts.insert(camel_to_snake(name.trim()), value);
    }
    assert!(block.contains("...HSL_NEUTRAL,"), "defaultSlidersRGB must spread HSL_NEUTRAL");
    assert!(
        common::IMG_PARAMETERS_TS.contains("HSL_KEYS.map((key) => [key, 0])"),
        "HSL_NEUTRAL must set every HSL slider to 0"
    );
    for name in ts_hsl_fields() {
        from_ts.insert(name, 0.0);
    }
    assert_eq!(
        from_ts.len(),
        SLIDER_COUNT,
        "parsed {} defaults, expected {SLIDER_COUNT}",
        from_ts.len()
    );

    let packed = open_darkroom_lib::export_rendering::sliders_to_bytes(&Default::default());
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "Sliders");

    for (i, field) in fields.iter().enumerate() {
        let rust = f32::from_le_bytes(packed[i * 4..i * 4 + 4].try_into().unwrap());
        let ts = from_ts
            .get(&field.name)
            .unwrap_or_else(|| panic!("{} missing from defaultSlidersRGB", field.name));
        assert_eq!(
            rust, *ts,
            "{}: Rust default is {rust}, frontend default is {ts}",
            field.name
        );
    }
}

#[test]
fn params_struct_matches_the_declared_size() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "Params");

    assert_eq!(
        struct_size(&fields),
        PARAMS_BYTES as usize,
        "PARAMS_BYTES disagrees with the WGSL Params struct: {fields:#?}"
    );
}

/// The curve fields are written from the CPU at a fixed offset, so that offset
/// has to land on `curves_active` in every language.
#[test]
fn curve_fields_sit_where_the_cpu_writes_them() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "Params");
    let at = fields
        .iter()
        .position(|f| f.name == "curves_active")
        .expect("curves_active missing from Params");
    assert_eq!(struct_size(&fields[..at]), CURVES_OFFSET as usize);

    let lut = fields.iter().find(|f| f.name == "curve_lut").expect("curve_lut missing");
    assert_eq!(lut.ty, format!("array<vec4f, {CURVE_LUT_ENTRIES}>"));

    assert!(
        CALC_PARAMS_PIPELINE_TS.contains(&format!("CURVES_OFFSET       = {CURVES_OFFSET};")),
        "CURVES_OFFSET in TypeScript disagrees with Rust"
    );
}

#[test]
fn sharp_params_struct_matches_the_declared_size() {
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "SharpParams");

    assert_eq!(
        struct_size(&fields),
        SHARP_PARAMS_BYTES as usize,
        "SHARP_PARAMS_BYTES disagrees with the WGSL SharpParams struct: {fields:#?}"
    );
}

#[test]
fn params_field_offsets_match_the_readback_accessors() {
    // `common::Params` hard-codes these offsets to decode the readback buffer.
    // If the struct is reordered, the accessors silently read the wrong lane —
    // unless this test catches it first.
    let fields = wgsl_struct(CALC_PARAMS_WGSL, "Params");

    let mut offset = 0usize;
    let mut offsets = std::collections::HashMap::new();
    for f in &fields {
        offsets.insert(f.name.clone(), offset / 4);
        offset += wgsl_size(&f.ty);
    }

    let expect = |name: &str, lane: usize| {
        assert_eq!(
            offsets.get(name).copied(),
            Some(lane),
            "{name} moved; update the accessors in tests/common/mod.rs"
        );
    };

    expect("dr_matrix", 0);
    expect("red_gamma", 16);
    expect("out_black", 19);
    expect("out_scale", 20);
    expect("wb_matrix", 24);
    expect("exposure", 36);
    expect("contrast", 37);
    expect("brightness", 38);
    expect("highlights", 39);
    expect("shadows", 40);
    expect("whites", 41);
    expect("blacks", 42);
    expect("hueSat_matrix", 44);
    expect("vibrance", 60);
}

// ── The develop chain, shared by three shaders ────────────────────────────────

/// Pull a delimited region out of a shader as normalised lines.
fn wgsl_region(source: &str, label: &str, start_marker: &str, end_marker: &str) -> Vec<String> {
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("{label}: start marker {start_marker:?} not found"));
    let end = source[start..]
        .find(end_marker)
        .map(|i| start + i + end_marker.len())
        .unwrap_or_else(|| panic!("{label}: end marker {end_marker:?} not found"));

    source[start..end]
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Steps 1-11 of the adjustment chain, which every chain shader must share.
fn develop_chain(source: &str, label: &str) -> Vec<String> {
    wgsl_region(
        source,
        label,
        "// 1. Darkroom matrix",
        "rgb = mix(vec3f(luma_vib), vib_rgb, 1.0 + vibranceAmount);",
    )
}

/// The chain lives in chunks/developChain.wgsl and is included, never copied.
#[test]
fn every_chain_shader_includes_the_one_develop_chain() {
    for (label, source) in CHAIN_SHADERS {
        assert_eq!(
            source.matches("fn develop(source: vec3f)").count(),
            1,
            "{label} should see the develop chain exactly once — a second copy \
             means it has been pasted in rather than included"
        );
        assert_eq!(
            source.matches("// 1. Darkroom matrix").count(),
            1,
            "{label} carries the chain's steps more than once"
        );
    }

    // And the one copy each of them sees is the same copy.
    let reference = develop_chain(DEVELOP_WGSL, "develop.wgsl");
    assert!(
        reference.len() > 20,
        "chain extraction looks wrong — only {} lines",
        reference.len()
    );

    for (label, source) in CHAIN_SHADERS {
        let chain = develop_chain(source, label);
        if chain != reference {
            let mut report = format!(
                "{label} no longer carries the same adjustment chain as develop.wgsl.\n\
                 It would describe an image the preview never renders.\n\n"
            );
            for (i, (a, b)) in reference.iter().zip(chain.iter()).enumerate() {
                if a != b {
                    report.push_str(&format!("line {i}:\n  chunk: {a}\n  {label}: {b}\n"));
                }
            }
            if chain.len() != reference.len() {
                report.push_str(&format!(
                    "\nlength differs: the chunk has {} lines, {label} has {}\n",
                    reference.len(),
                    chain.len()
                ));
            }
            panic!("{report}");
        }
    }
}

// ── The colorimetry the colour-space tools measure in ─────────────────────────

/// Linear RGB → XYZ for a set of primaries and a white point, column-major.
/// Derived independently of outputSpace.ts and output_space.rs on purpose.
fn rgb_to_xyz_column_major(primaries: [[f64; 2]; 3], white: [f64; 2]) -> [f64; 9] {
    let mut m = [0.0; 9];
    for (col, [x, y]) in primaries.iter().enumerate() {
        m[col * 3] = x / y;
        m[col * 3 + 1] = 1.0;
        m[col * 3 + 2] = (1.0 - x - y) / y;
    }

    let w = [
        white[0] / white[1],
        1.0,
        (1.0 - white[0] - white[1]) / white[1],
    ];
    let inverse = invert_column_major(m);
    let scale: [f64; 3] = std::array::from_fn(|col| {
        (0..3).map(|row| inverse[row * 3 + col] * w[row]).sum()
    });

    for col in 0..3 {
        for row in 0..3 {
            m[col * 3 + row] *= scale[col];
        }
    }
    m
}

/// A 3×3 inverse by cofactors, in and out of column-major.
fn invert_column_major(m: [f64; 9]) -> [f64; 9] {
    let at = |col: usize, row: usize| m[col * 3 + row];

    let mut cofactors = [0.0; 9];
    for col in 0..3 {
        for row in 0..3 {
            let (c0, c1) = ((col + 1) % 3, (col + 2) % 3);
            let (r0, r1) = ((row + 1) % 3, (row + 2) % 3);
            // Transposed on the way out, which is what makes this the inverse
            // rather than the adjugate.
            cofactors[row * 3 + col] =
                at(c0, r0) * at(c1, r1) - at(c0, r1) * at(c1, r0);
        }
    }

    // Expanded along the first column, which in column-major order is the
    // first three entries.
    let det: f64 = (0..3).map(|row| at(0, row) * cofactors[row * 3]).sum();
    assert!(det.abs() > 1e-12, "matrix is singular");
    cofactors.map(|v| v / det)
}

/// Pull a `const NAME = mat3x3<f32>( vec3f(..), vec3f(..), vec3f(..) );` out of
/// WGSL as a column-major array of nine floats.
fn wgsl_mat3(source: &str, name: &str) -> [f64; 9] {
    let body = source
        .split(&format!("const {name} = mat3x3<f32>("))
        .nth(1)
        .unwrap_or_else(|| panic!("no {name} declaration"))
        .split(");")
        .next()
        .unwrap();

    let values: Vec<f64> = body
        .split("vec3f(")
        .skip(1)
        .flat_map(|col| {
            col.split(')')
                .next()
                .unwrap()
                .split(',')
                .map(|t| t.trim().parse().expect("matrix component is not a float"))
                .collect::<Vec<f64>>()
        })
        .collect();

    assert_eq!(values.len(), 9, "{name}: expected nine components");
    values.try_into().unwrap()
}

/// The working space ↔ XYZ constants in chunks/colorimetry.wgsl must follow the
/// working primaries.
#[test]
fn the_colorimetry_constants_match_the_working_primaries() {
    use open_darkroom_lib::icc::{D65, WORKING_PRIMARIES};

    let derived = rgb_to_xyz_column_major(WORKING_PRIMARIES, D65);
    let declared = wgsl_mat3(COLOR_SPACE_WGSL, "M_WORKING_TO_XYZ");
    for i in 0..9 {
        assert!(
            (declared[i] - derived[i]).abs() < 5e-7,
            "M_WORKING_TO_XYZ[{i}]: the chunk says {}, the primaries give {}",
            declared[i],
            derived[i]
        );
    }

    let inverse = invert_column_major(derived);
    let declared = wgsl_mat3(COLOR_SPACE_WGSL, "M_XYZ_TO_WORKING");
    for i in 0..9 {
        assert!(
            (declared[i] - inverse[i]).abs() < 5e-7,
            "M_XYZ_TO_WORKING[{i}]: the chunk says {}, the inverse gives {}",
            declared[i],
            inverse[i]
        );
    }
}

/// The diagram and the statistics read the same `GamutOverlay` description.
#[test]
fn the_gamut_overlay_is_described_once() {
    for (label, source) in [
        ("colorSpace.wgsl", COLOR_SPACE_WGSL),
        ("chromaticityDraw.wgsl", CHROMATICITY_DRAW_WGSL),
    ] {
        assert_eq!(
            source.matches("struct GamutOverlay {").count(),
            1,
            "{label} declares the gamut overlay more than once"
        );
        assert!(
            source.contains("fn inside_gamut"),
            "{label} no longer sees the containment test — check its includes"
        );
        assert!(
            source.contains(": GamutOverlay;"),
            "{label} includes the overlay without binding it"
        );
    }
}

// ── The include resolver ──────────────────────────────────────────────────────

/// Every shader reaching this test came through build.rs, so an unresolved
/// directive means the resolver silently skipped one — and WGSL would see a
/// comment where a declaration should be.
#[test]
fn no_shader_still_carries_an_unresolved_include() {
    for (label, source) in SHADERS {
        assert!(
            !source.contains("//!include"),
            "{label} still carries an unresolved include directive"
        );
    }
}

/// The tone maths used to live twice, guarded by a test comparing the copies.
/// There is one copy now, so what needs proving is that the shaders that used
/// to carry it still see it.
#[test]
fn the_shaders_that_need_the_tone_maths_get_it() {
    // The transfer functions, which the output transform needs too: it undoes
    // the working curve before the gamut matrix.
    for (label, source) in [
        ("develop.wgsl", DEVELOP_WGSL),
        ("histogram.wgsl", HISTOGRAM_WGSL),
        ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
    ] {
        for needle in [
            "TONE_PIVOT",
            "CONTRAST_BASE",
            "fn contrast_curve",
            "fn encode_srgb",
            "fn decode_srgb",
        ] {
            assert!(
                source.contains(needle),
                "{label} no longer sees {needle} — check its includes"
            );
        }
    }

    // The luminance weights are a separate chunk. The two chain shaders read
    // them for vibrance, and the output transform reads them to find the
    // neutral its gamut compression aims at.
    for (label, source) in [
        ("develop.wgsl", DEVELOP_WGSL),
        ("histogram.wgsl", HISTOGRAM_WGSL),
        ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
    ] {
        assert!(
            source.contains("const LUMA"),
            "{label} no longer sees const LUMA — check its includes"
        );
    }
}

/// `LUMA` and calcParams' `LR`/`LG`/`LB` are the same luminance vector of the
/// same working space, declared twice because one is a chunk the adjustment
/// shaders include and the other feeds a matrix calcParams builds on the GPU.
///
/// They have disagreed before: the working space moved to Rec.2020 and only
/// calcParams followed, so the saturation matrix collapsed a colour to one grey
/// while vibrance mixed it towards another. Neither looks wrong on a neutral,
/// which is what every render with default sliders is.
#[test]
fn the_two_luminance_vectors_describe_the_same_working_space() {
    let weights = |source: &str, names: [&str; 3]| -> [f32; 3] {
        names.map(|name| {
            source
                .split(&format!("{name}: f32 = "))
                .nth(1)
                .unwrap_or_else(|| panic!("no {name} declaration"))
                .split(';')
                .next()
                .unwrap()
                .trim()
                .parse()
                .unwrap_or_else(|e| panic!("{name} is not a float: {e}"))
        })
    };

    let constants = weights(CALC_PARAMS_WGSL, ["LR", "LG", "LB"]);

    let chunk: Vec<f32> = DEVELOP_WGSL
        .split("const LUMA = vec3f(")
        .nth(1)
        .expect("develop.wgsl declares no const LUMA")
        .split(')')
        .next()
        .unwrap()
        .split(',')
        .map(|t| t.trim().parse().expect("LUMA component is not a float"))
        .collect();

    assert_eq!(chunk.len(), 3, "expected three components, got {chunk:?}");
    for i in 0..3 {
        assert_eq!(
            chunk[i], constants[i],
            "luma component {i}: chunks/luma.wgsl says {}, calcParams.wgsl says {}",
            chunk[i], constants[i]
        );
    }

    // And both are the working space's, not the sRGB space the chain used to
    // run in — equal-but-wrong is the other way this drifts.
    const BT2020: [f32; 3] = [0.2627002, 0.6779981, 0.0593017];
    assert_eq!(chunk[..], BT2020[..], "the weights are not BT.2020's");
}

/// The preview is rendered through the output transform and the histogram is
/// measured through it, so the two have to be the same transform. They were
/// briefly not: the histogram binned the working space while the screen showed
/// the destination, and the two disagreed by twenty code values on saturated
/// content without either looking obviously wrong.
#[test]
fn the_preview_and_the_histogram_share_one_output_transform() {
    for (label, source) in [
        ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
        ("histogram.wgsl", HISTOGRAM_WGSL),
    ] {
        for needle in ["struct OutputSpace", "fn to_destination", "fn gamut_compress"] {
            assert!(
                source.contains(needle),
                "{label} no longer sees {needle} — check its includes"
            );
        }
        assert_eq!(
            source.matches("struct OutputSpace").count(),
            1,
            "{label} declares the output space more than once"
        );
        assert!(
            source.contains(": OutputSpace;"),
            "{label} includes the output transform without binding its uniform, \
             so it would read a zero matrix"
        );
    }
}

/// The develop stage must not know where the render is going.
///
/// Vibrance measures how saturated a pixel already looks, and it measures in
/// sRGB on purpose: a measurement that followed the destination would re-grade
/// the picture when the export format changed, so the same sliders would stop
/// meaning the same image. The reference matrix therefore lives with the other
/// measurement constants, and develop.wgsl does not include the transform at
/// all.
#[test]
fn the_develop_stage_measures_saturation_against_a_fixed_reference() {
    for (label, source) in [("develop.wgsl", DEVELOP_WGSL), ("histogram.wgsl", HISTOGRAM_WGSL)] {
        assert_eq!(
            source.matches("const M_WORKING_TO_SRGB").count(),
            1,
            "{label} should see the vibrance reference matrix exactly once"
        );
    }

    assert!(
        !DEVELOP_WGSL.contains("fn to_destination"),
        "develop.wgsl has picked up the output transform — the develop chain must \
         render the same image whatever space the export is written in"
    );
}

/// The structs are declared in a chunk and bound per shader, so a shader can
/// lose the declaration while still naming the type.
#[test]
fn every_shader_that_binds_a_struct_also_declares_it() {
    for (label, source) in SHADERS {
        for name in ["Params", "SharpParams", "SourceSpace", "OutputSpace"] {
            if !source.contains(&format!(": {name};")) {
                continue;
            }
            assert!(
                source.contains(&format!("struct {name} {{")),
                "{label} binds {name} without declaring it — check its includes"
            );
        }
    }
}

// ── The output spaces, declared in three languages ────────────────────────────

/// Pull `name: f32 = <number>;` style WGSL constants out of a shader.
fn wgsl_const(source: &str, name: &str) -> f64 {
    source
        .split(&format!("{name}: u32 = "))
        .nth(1)
        .unwrap_or_else(|| panic!("no {name} declaration"))
        .split(|c| c == 'u' || c == ';')
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("{name} is not a number: {e}"))
}

/// The chromaticities are the one set of colour numbers stated in both
/// languages — everything else about a space is derived from them. Derived
/// separately on each side, so a typo in either would give the preview and the
/// exported file different primaries while both looked plausible.
#[test]
fn typescript_declares_the_same_primaries_as_rust() {
    // Each entry is `"id": [[x, y], [x, y], [x, y]],` inside the PRIMARIES map.
    let table = OUTPUT_SPACE_TS
        .split("const PRIMARIES")
        .nth(1)
        .expect("outputSpace.ts declares no PRIMARIES");

    for space in OutputSpace::ALL {
        let id = serde_json::to_string(&space).expect("space serialises");
        let row = table
            .split(&format!("{id}:"))
            .nth(1)
            .unwrap_or_else(|| panic!("outputSpace.ts has no primaries for {id}"))
            .split(']')
            .take(4)
            .collect::<Vec<_>>()
            .join("]");

        let numbers: Vec<f64> = row
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .filter(|t| !t.is_empty())
            .map(|t| t.parse().expect("primary is not a number"))
            .collect();

        let expected: Vec<f64> = space.primaries().concat();
        assert_eq!(
            numbers.len(),
            6,
            "expected six chromaticities for {id}, parsed {numbers:?}"
        );
        for (i, (&got, &want)) in numbers.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                got, want,
                "{id} chromaticity {i}: outputSpace.ts says {got}, output_space.rs says {want}"
            );
        }
    }
}

/// The curve id is an integer crossing two language boundaries on its way to the
/// shader, and nothing about a wrong one looks wrong: the image still renders,
/// just through the wrong transfer function, which reads as a contrast shift
/// rather than as a bug.
#[test]
fn the_transfer_curve_ids_agree_across_the_three_languages() {
    for (wgsl_name, ts_name, rust) in [
        ("TRANSFER_SRGB", "srgb", Transfer::Srgb),
        ("TRANSFER_ADOBE", "adobe", Transfer::Adobe),
        ("TRANSFER_BT2020", "bt2020", Transfer::Bt2020),
    ] {
        let id = rust as u32 as f64;

        assert_eq!(
            wgsl_const(COLOR_SPACE_ENCODE_WGSL, wgsl_name),
            id,
            "{wgsl_name} in the shader disagrees with output_space.rs"
        );

        let ts: f64 = OUTPUT_SPACE_TS
            .split("const TRANSFER = {")
            .nth(1)
            .expect("outputSpace.ts declares no TRANSFER")
            .split(&format!("{ts_name}:"))
            .nth(1)
            .unwrap_or_else(|| panic!("outputSpace.ts has no {ts_name} curve"))
            .split(',')
            .next()
            .unwrap()
            .trim()
            .parse()
            .expect("curve id is not a number");
        assert_eq!(
            ts, id,
            "TRANSFER.{ts_name} in TypeScript disagrees with output_space.rs"
        );
    }
}

/// The space travels from the export menu to the backend as a string, so a
/// rename on one side is a failed export rather than a wrong colour — but only
/// at run time, in the one code path nobody exercises until they export.
#[test]
fn typescript_and_rust_agree_on_the_output_space_names() {
    let union = OUTPUT_SPACE_TS
        .split("export type OutputSpaceId =")
        .nth(1)
        .expect("outputSpace.ts declares no OutputSpaceId")
        .split(';')
        .next()
        .unwrap();

    let names: Vec<&str> = union
        .split('|')
        .map(|t| t.trim().trim_matches('"'))
        .collect();

    assert_eq!(
        names.len(),
        OutputSpace::ALL.len(),
        "OutputSpaceId lists {} spaces, output_space.rs has {}",
        names.len(),
        OutputSpace::ALL.len()
    );

    for name in &names {
        let json = format!("\"{name}\"");
        serde_json::from_str::<OutputSpace>(&json).unwrap_or_else(|e| {
            panic!("the backend cannot deserialise the frontend's {json}: {e}")
        });
    }

    // And every space the backend knows is offered, so none is unreachable.
    for space in OutputSpace::ALL {
        let id = serde_json::to_string(&space).expect("space serialises");
        assert!(
            names.contains(&id.trim_matches('"')),
            "output_space.rs has {id} but OutputSpaceId does not offer it"
        );
    }
}

/// The uniform is written by the frontend and by the export path, into the same
/// struct, so its size has to agree the way the other buffers' do.
#[test]
fn typescript_declares_the_same_output_space_size_as_rust() {
    let declared: u64 = OUTPUT_SPACE_BUFFER_TS
        .split("OUTPUT_SPACE_BUFFER_SIZE = ")
        .nth(1)
        .expect("outputSpaceBuffer.ts declares no OUTPUT_SPACE_BUFFER_SIZE")
        .split(';')
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("expected a numeric literal");

    assert_eq!(
        declared, OUTPUT_SPACE_BYTES,
        "OUTPUT_SPACE_BUFFER_SIZE in TypeScript disagrees with Rust"
    );
    assert_eq!(
        OUTPUT_SPACE_BYTES % 16,
        0,
        "a uniform struct binds 16-aligned"
    );
}

/// The frontend picks the space per format; the backend only does as it is told.
/// This pins the rule itself, which is the part that would otherwise drift
/// silently into shipping 16-bit files in sRGB's gamut.
#[test]
fn the_sixteen_bit_default_is_the_working_space() {
    let rule = EXPORT_SETTINGS_TS
        .split("export function defaultOutputSpace")
        .nth(1)
        .expect("exportSettings.ts declares no defaultOutputSpace");

    assert!(
        rule.contains("WORKING_SPACE"),
        "the 16-bit TIFF default is no longer the working space"
    );
    assert!(
        rule.contains("\"srgb\""),
        "the 8-bit default is no longer sRGB"
    );

    // The working space the frontend names has to be the one the chain runs in.
    let working = OUTPUT_SPACE_TS
        .split("export const WORKING_SPACE: OutputSpaceId = ")
        .nth(1)
        .expect("outputSpace.ts declares no WORKING_SPACE")
        .split(';')
        .next()
        .unwrap()
        .trim()
        .trim_matches('"');
    assert_eq!(
        serde_json::from_str::<OutputSpace>(&format!("\"{working}\"")).expect("a known space"),
        OutputSpace::Rec2020,
        "the frontend's working space is not Rec.2020"
    );
}
