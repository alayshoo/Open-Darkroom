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
    CALC_PARAMS_PIPELINE_TS, EXPORT_RENDERING_RS, HISTOGRAM_WGSL, OPEN_IMAGE_TS, WORKING_SPACE_TS,
};

use open_darkroom_lib::export_rendering::{
    CALC_PARAMS_WGSL, CALC_SHARP_TEXTURE_WGSL, COLOR_SPACE_ENCODE_WGSL, COMPOSITE_WGSL,
    DEVELOP_WGSL, PARAMS_BYTES, SHARP_PARAMS_BYTES, SLIDERS_BYTES, SLIDER_COUNT,
    SOURCE_SPACE_BYTES, VIEW_BYTES,
};
use open_darkroom_lib::icc::SRGB_TO_WORKING;
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

/// Every shader the export path compiles, flattened by build.rs.
const SHADERS: [(&str, &str); 6] = [
    ("calcParams.wgsl", CALC_PARAMS_WGSL),
    ("develop.wgsl", DEVELOP_WGSL),
    ("calcSharpTexture.wgsl", CALC_SHARP_TEXTURE_WGSL),
    ("composite.wgsl", COMPOSITE_WGSL),
    ("colorSpaceEncode.wgsl", COLOR_SPACE_ENCODE_WGSL),
    ("histogram.wgsl", HISTOGRAM_WGSL),
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
        "vec3f" | "vec4f" => 16,
        // A mat3x3 has 16-byte column stride, so it occupies 48 bytes, not 36.
        "mat3x3<f32>" => 48,
        "mat4x4<f32>" => 64,
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

    let ts: Vec<String> = accessed_fields(CALC_PARAMS_PIPELINE_TS, "function slidersToArray", '[', ']')
        .iter()
        .map(|n| camel_to_snake(n))
        .collect();

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

// ── The develop chain, duplicated in two shaders ──────────────────────────────

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

/// Pull the shared adjustment chain out of a shader: everything from the first
/// numbered step through the linear→sRGB conversion.
/// Steps 1-10 of the adjustment chain — everything the two shaders have to
/// agree on. What each does afterwards is its own business: develop hands the
/// perceptual signal to composite.wgsl unclamped, while histogram clamps it and
/// bins it.
fn develop_chain(source: &str, label: &str) -> Vec<String> {
    wgsl_region(
        source,
        label,
        "// 1. Darkroom matrix",
        "rgb = mix(vec3f(luma_vib), vib_rgb, 1.0 + vibranceAmount);",
    )
}

#[test]
fn develop_and_histogram_apply_the_same_chain() {
    let develop = develop_chain(DEVELOP_WGSL, "develop.wgsl");
    let histogram = develop_chain(HISTOGRAM_WGSL, "histogram.wgsl");

    assert!(
        develop.len() > 20,
        "chain extraction looks wrong — only {} lines",
        develop.len()
    );

    if develop != histogram {
        let mut report = String::from(
            "develop.wgsl and histogram.wgsl no longer apply the same adjustment chain.\n\
             The histogram would describe an image the preview never renders.\n\n",
        );
        for (i, (d, h)) in develop.iter().zip(histogram.iter()).enumerate() {
            if d != h {
                report.push_str(&format!("line {i}:\n  develop:   {d}\n  histogram: {h}\n"));
            }
        }
        if develop.len() != histogram.len() {
            report.push_str(&format!(
                "\nlength differs: develop has {} lines, histogram has {}\n",
                develop.len(),
                histogram.len()
            ));
        }
        panic!("{report}");
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

    // The luminance weights are a separate chunk, and only the two shaders that
    // run the adjustment chain read them.
    for (label, source) in [("develop.wgsl", DEVELOP_WGSL), ("histogram.wgsl", HISTOGRAM_WGSL)] {
        assert!(
            source.contains("const LUMA"),
            "{label} no longer sees const LUMA — check its includes"
        );
    }
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
        for needle in ["const M_WORKING_TO_DESTINATION", "fn to_destination"] {
            assert!(
                source.contains(needle),
                "{label} no longer sees {needle} — check its includes"
            );
        }
        assert_eq!(
            source.matches("const M_WORKING_TO_DESTINATION").count(),
            1,
            "{label} declares the destination matrix more than once"
        );
    }
}

/// The structs are declared in a chunk and bound per shader, so a shader can
/// lose the declaration while still naming the type.
#[test]
fn every_shader_that_binds_a_struct_also_declares_it() {
    for (label, source) in SHADERS {
        for name in ["Params", "SharpParams", "SourceSpace"] {
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
