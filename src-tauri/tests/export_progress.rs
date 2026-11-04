// src-tauri/tests/export_progress.rs
//
// The bar the export modal shows: phases in order, never moving backwards, and
// encoding given the room its cost asks for.

use std::time::Duration;

use open_darkroom_lib::export_encoding::{ExportFormat, ExportSettings};
use open_darkroom_lib::export_progress::{encode_share, Phase, ProgressPlan};
use open_darkroom_lib::output_space::OutputSpace;

fn settings(format: ExportFormat) -> ExportSettings {
    ExportSettings {
        format,
        output_space: OutputSpace::Srgb,
        png_compression: 6,
        jpeg_quality: 90,
        webp_lossless: true,
        webp_quality: 90,
        webp_compression: 6,
        tiff_bit_depth: 8,
        jxl_lossless: false,
        jxl_quality: 99,
        jxl_effort: 7,
        jxl_bit_depth: 16,
        is_bw: false,
    }
}

#[test]
fn encoding_gets_the_share_of_the_bar_its_time_asks_for() {
    assert_eq!(encode_share(1.0, 0.0), 0.0);
    assert!((encode_share(1.0, 3.0) - 0.75).abs() < 1e-6);
    assert!(encode_share(0.0, 5.0) < 1.0, "the render must always keep some of the bar");
    assert_eq!(encode_share(0.0, 0.0), 0.0);
}

#[test]
fn a_slow_encode_stops_the_render_short_of_the_end() {
    // One second of rendering against three of encoding.
    let plan = ProgressPlan::new(3.0);
    assert_eq!(plan.preparing().unwrap().phase, Phase::Preparing);

    plan.rendered_after(0.0, Duration::ZERO);
    let quarter = plan.rendered_after(0.25, Duration::from_millis(250)).unwrap();
    assert!((quarter.progress - 0.0625).abs() < 1e-4, "{quarter:?}");

    let done = plan.rendered_after(1.0, Duration::from_secs(1)).unwrap();
    assert_eq!(done.phase, Phase::Rendering);
    assert!((done.progress - 0.25).abs() < 1e-4, "render should end at 25%: {done:?}");

    let encoding = plan.encoding(true).unwrap();
    assert_eq!(encoding.phase, Phase::Encoding);
    assert!(!encoding.indeterminate);

    let half = plan.encoded(0.5).unwrap();
    assert!((half.progress - 0.625).abs() < 1e-4, "{half:?}");
    assert_eq!(plan.encoded(1.0).unwrap().progress, 1.0);
}

#[test]
fn an_encoder_without_progress_is_shown_as_indeterminate() {
    let plan = ProgressPlan::new(1.0);
    plan.rendered_after(0.0, Duration::ZERO);
    plan.rendered_after(1.0, Duration::from_secs(1));

    let event = plan.encoding(false).unwrap();
    assert_eq!(event.phase, Phase::Encoding);
    assert!(event.indeterminate);
    assert!((event.progress - 0.5).abs() < 1e-4, "{event:?}");
}

#[test]
fn progress_never_goes_backwards_and_skips_tiny_steps() {
    let plan = ProgressPlan::new(0.5);
    plan.rendered_after(0.0, Duration::ZERO);
    let first = plan.rendered_after(0.5, Duration::from_millis(500)).unwrap();

    assert!(plan.rendered_after(0.4, Duration::from_millis(600)).is_none(), "went backwards");
    assert!(plan.rendered_after(0.501, Duration::from_millis(510)).is_none(), "too small to send");

    let later = plan.rendered_after(0.9, Duration::from_millis(900)).unwrap();
    assert!(later.progress > first.progress);
}

#[test]
fn only_png_and_tiff_report_encode_progress() {
    for (format, reports) in [
        (ExportFormat::Png, true),
        (ExportFormat::Tiff, true),
        (ExportFormat::Jpeg, false),
        (ExportFormat::Webp, false),
        (ExportFormat::Jxl, false),
    ] {
        assert_eq!(settings(format).reports_encode_progress(), reports, "{format:?}");
    }
}

#[test]
fn the_cost_model_ranks_the_encoders_sensibly() {
    let cost = |s: ExportSettings| s.encode_seconds_per_megapixel();

    let tiff = cost(settings(ExportFormat::Tiff));
    let jpeg = cost(settings(ExportFormat::Jpeg));
    let png = cost(settings(ExportFormat::Png));
    let webp_lossless = cost(settings(ExportFormat::Webp));
    assert!(tiff < jpeg && jpeg < png && png < webp_lossless);

    let mut lossless = settings(ExportFormat::Jxl);
    lossless.jxl_lossless = true;
    let mut slower = settings(ExportFormat::Jxl);
    slower.jxl_effort = 9;
    assert!(cost(settings(ExportFormat::Jxl)) < cost(slower));

    let mut twelve = settings(ExportFormat::Jxl);
    twelve.jxl_lossless = true;
    twelve.jxl_bit_depth = 12;
    let mut eight = twelve.clone();
    eight.jxl_bit_depth = 8;
    assert!(cost(eight) < cost(twelve.clone()) && cost(twelve) < cost(lossless));
}

#[test]
fn any_jxl_depth_above_eight_renders_at_sixteen() {
    for depth in 8u8..=16 {
        let mut s = settings(ExportFormat::Jxl);
        s.jxl_bit_depth = depth;
        assert_eq!(s.bit_depth(), if depth == 8 { 8 } else { 16 }, "{depth}-bit");
    }
}
