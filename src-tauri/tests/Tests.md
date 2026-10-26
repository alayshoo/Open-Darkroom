# Test suite

Three layers. **L1** is CPU-only and fast. **L2** runs the shipped WGSL headlessly
through wgpu — it is the numerical oracle. **L3** runs the frontend's own modules in
a real browser, only to prove the TypeScript wiring agrees with L2.

| Where | Run with |
|---|---|
| `src-tauri/tests/` — L1 + L2 (Rust) | `cargo test` |
| `tests/unit/` — L1 (TypeScript) | `pnpm test:unit` |
| `tests/browser/` — L3 (WebGPU) | `pnpm test:browser` |

L2 and L3 need a GPU. L3 runs **headed** — headless Chromium returns no WebGPU adapter.

Nothing is `#[ignore]`d. 271 Rust tests, 152 TypeScript, 40 in-browser.

---

## The render chain

Six passes, and the tests are split along the same seams.

```
calcParams.wgsl        compute — sliders + view + tone map → Params, SharpParams
develop.wgsl           steps 1-12, into the perceptual working space
calcSharpTexture.wgsl  h_main — luma, blurred along x at four scales
calcSharpTexture.wgsl  v_main — blurs finished, three bands subtracted out
composite.wgsl         the bands applied by scaling the colour
colorSpaceEncode.wgsl  output transform + gamut compression (chunks/outputTransform.wgsl)
```

`develop.wgsl` deliberately does **not** clamp: the headroom has to survive as far
as the output transform. Anything asserting on final pixel values must therefore
drive the whole chain, not the develop pass alone.

The output transform reads its destination from a uniform — the display for a
preview, the chosen space for an export, both at once for a soft proof — so a
test asserting on final pixel values has to say which space it means.
`develop.wgsl` does not see that uniform at all: the develop chain must render
the same image whatever the export is written in.

The two blur passes are skipped whenever no band can reach the screen. That is
only ever a saving — `calcParams.wgsl` zeroes the amounts over the same condition,
so composite is a no-op regardless of what the detail texture holds.

## L1 — `shader_contracts.rs` (27)

Reads source files as text. Catches a field added in one language and not the others.

| Test | Asserts |
|---|---|
| `sliders_struct_is_all_f32_and_matches_the_declared_size` | 30 f32 fields = 120 bytes |
| `rust_slider_packing_matches_the_wgsl_struct` | `sliders_to_bytes` order == WGSL order |
| `typescript_slider_packing_matches_the_wgsl_struct` | `slidersToArray` order == WGSL order |
| `typescript_declares_the_same_buffer_sizes_as_rust` | TS buffer-size constants == Rust |
| `view_struct_is_all_f32_and_matches_the_declared_size` | `View` is f32-only and 16 bytes |
| `tone_map_struct_is_all_f32_and_matches_the_declared_size` | `ToneMap` is f32-only and 16 bytes |
| `rust_default_sliders_match_the_frontend_defaults` | `SlidersPayload::default()` == `defaultSlidersRGB` |
| `params_struct_is_identical_across_every_shader` | `Params` same in all 3 WGSL files |
| `params_struct_matches_the_declared_size` | `Params` = 256 bytes |
| `sharp_params_struct_is_identical_across_every_shader` | `SharpParams` same in all 3 WGSL files |
| `sharp_params_struct_matches_the_declared_size` | `SharpParams` = 48 bytes |
| `params_field_offsets_match_the_readback_accessors` | test harness offsets still valid |
| `develop_and_histogram_apply_the_same_chain` | steps 1-11 have not drifted between the two copies |
| `develop_and_histogram_share_the_same_tone_maths` | so has the duplicated tone maths they call |
| `typescript_matches_the_cancelled_open_sentinel` | the string that tells a cancelled open from a failed one |
| `the_preview_and_the_histogram_share_one_output_transform` | both see the struct, the transform and the compression, and bind the uniform |
| `the_develop_stage_measures_saturation_against_a_fixed_reference` | vibrance measures in sRGB; develop never sees the destination |
| `typescript_declares_the_same_primaries_as_rust` | the one set of chromaticities both sides derive from |
| `the_transfer_curve_ids_agree_across_the_three_languages` | WGSL == Rust == TypeScript |
| `typescript_and_rust_agree_on_the_output_space_names` | every id the menu offers deserialises, and none is unreachable |
| `typescript_declares_the_same_output_space_size_as_rust` | the uniform is 160 bytes on both sides |
| `the_sixteen_bit_default_is_the_working_space` | 8-bit defaults to sRGB, 16-bit TIFF to Rec.2020 |

The chain comparison stops after vibrance, because that is where the two files
stop agreeing by design — develop hands off to `composite.wgsl` unclamped, while
histogram clamps and bins.

## L1 — `color.rs` (15)

Two tables, one curve. The preview uploads `rgba16float`, the export
`rgba32float` — see [below](#why-the-export-uploads-twice-as-wide-as-the-preview).

| Test | Asserts |
|---|---|
| `lut_spans_the_domain_and_is_monotonic` | 65536 entries, 0→0, 1→1, non-decreasing |
| `lut_tracks_the_reference_curve` | matches an independent f64 sRGB curve |
| `f16_storage_survives_a_round_trip_to_8_bit` | f16 costs < 0.5 of 255 |
| `linearize_produces_eight_bytes_per_pixel` | output sizing |
| `linearize_maps_each_channel_through_the_lut` | no channel crossing; alpha not gamma-decoded |
| `linearize_is_order_independent` | rayon parallelism has no data race |
| `linearize_widens_three_channels_to_an_opaque_rgba` | RGB and opaque RGBA upload identically |
| `a_three_channel_upload_is_fully_opaque` | the synthesised alpha is exactly 1.0 |
| `the_f32_lut_spans_the_domain_and_is_strictly_increasing` | all 65536 codes stay distinct |
| `the_f32_lut_resolves_highlights_the_f16_one_collapses` | the top octave, measured against f16 |
| `the_two_tables_describe_the_same_curve` | f16 and f32 tables agree to one f16 ULP |
| `linearize_f32_produces_sixteen_bytes_per_pixel` | output sizing |
| `linearize_f32_maps_each_channel_through_the_lut` | no channel crossing; alpha not gamma-decoded |
| `linearize_f32_is_order_independent` | rayon parallelism has no data race |
| `linearize_f32_widens_three_channels_to_an_opaque_rgba` | RGB and opaque RGBA upload identically |

### Why a source can be three channels wide

A file whose format declares no alpha — every JPEG, and RAW when it lands — is
decoded and held as RGB, a quarter less memory resident for as long as the image
is open. The GPU still wants RGBA, so the alpha is synthesised at the point of
upload rather than carried through the buffer. Everything downstream of the
decode takes the sample count as a parameter, and the tests that matter are the
equivalence ones: the same pixels, stored either way, must come out the same.

### Why the export uploads twice as wide as the preview

An f16 mantissa is 10 bits, so above linear 0.5 the spacing is 4.9e-4. One
16-bit sRGB code near white is 3.5e-5 in linear light — fourteen times finer —
so an f16 upload collapses runs of about seventeen adjacent input codes onto a
single value, and 65536 input codes reach the shader as roughly 11k. That
ceiling followed the signal all the way into the 16-bit TIFF, which carried
about 11 real bits.

The export therefore uploads `rgba32float` and renders to `rgba32float`, keeping
every code distinct end to end. The preview keeps the f16 table: it renders to
an 8-bit display, where the dropped bits cannot be seen and the halved VRAM can.
Two tables mean two things to keep in step, which is what
`the_two_tables_describe_the_same_curve` is for.

## L1 + L2 — `sharpness.rs` (31)

The sharpness block, from the UI type down to the halo on a rendered edge. Three
bands out of four blurs: the unsharp mask is a high-pass at the user's radius,
texture a band-pass between two fixed scales, and clarity the band directly above
texture's, up to a radius proportional to the frame.

Texture and clarity share a cutoff, so between them they partition the range of
scales rather than overlapping — pushing both sliders cannot count the same
detail twice, and clarity cannot lift film grain. The unsharp mask deliberately
does overlap both: it is the explicit sharpening control.

| Test | Asserts |
|---|---|
| `sharpness_defaults_are_neutral` | the six defaults switch the effect off |
| `sharpness_lands_in_its_own_lanes` | each slider reaches its own f32 lane |
| `sharpness_does_not_shift_the_lanes_before_it` | the 24 earlier lanes are undisturbed |
| `the_uniform_is_padded_out_to_its_bound_size` | 120 bytes of fields, bound at 128 |
| `the_export_payload_carries_the_sharpness_keys` | camelCase JSON reaches `SlidersPayload` |
| `sharpness_does_not_change_the_computed_params` | it has its own buffer; `Params` is untouched |
| `the_amount_is_off_until_the_slider_is_moved` | the default mask is switched off |
| `an_amount_of_one_hundred_percent_adds_the_whole_band` | the amount scale is 1:1 |
| `the_radius_is_measured_in_full_resolution_pixels` | sigma scales with the render scale |
| `zooming_out_fades_the_mask_instead_of_aliasing` | sigma floored, amount faded to zero |
| `the_thresholds_are_scaled_out_of_slider_units` | 0 is an exact no-op at both rails |
| `the_mask_overshoots_on_both_sides_of_an_edge` | the halo, and flats that do not move |
| `a_negative_amount_softens_the_same_edge` | the overshoot reverses |
| `a_wider_radius_reaches_further_from_the_edge` | radius controls the reach |
| `sharpening_does_not_shift_hue` | channel ratios survive; the band scales, not adds |
| `amount_zero_renders_exactly_as_if_the_mask_were_absent` | bit-exact no-op |
| `texture_acts_on_mid_scale_detail` | the band responds at its own scale |
| `texture_passes_over_the_finest_detail` | the lower cutoff; grain is left where USM lifts it |
| `texture_reverses_with_the_slider` | both directions |
| `clarity_steepens_a_broad_edge` | reach, ten pixels out from the edge |
| `clarity_radius_scales_with_the_image_between_its_clamps` | fraction of the short edge, clamped |
| `clarity_backs_off_at_the_endpoints` | the midtone mask protects the extremes |
| `clarity_saturates_rather_than_growing_without_bound` | the soft clip |
| `clarity_and_texture_act_on_scales_the_other_cannot` | the bands partition, in both directions |
| `clarity_and_texture_are_separate_channels` | no crossed wires in the detail texture |
| `chunk_size_does_not_change_a_sharpened_render` | 5 chunk sizes vs one pass, no seam |
| `chunk_size_does_not_change_a_clarity_render` | the same at clarity's much wider kernel |
| `the_overlap_is_one_radius_and_disappears_when_the_mask_is_off` | margin follows the widest band |
| `the_view_defaults_to_full_resolution` | `render_scale` defaults to 1.0 |
| `the_view_packs_the_render_scale_into_the_first_lane` | scale and both dimensions, in order |
| `the_view_zeroes_its_tail_padding` | no uninitialised bytes in the uniform |

### Why the export renders more rows than it keeps

A blur reads across chunk boundaries, so each chunk is rendered with a margin of
neighbouring rows that is then cropped out of the readback. The margin is **one**
radius, not two: the vertical pass at row `y` reads the horizontal pass at
`y ± radius`, and the horizontal pass reads only its own row.

At the top and bottom of the image the margin is naturally short, which is
correct — a single-pass render clamps at those edges too, so the results agree.
`chunk_size_does_not_change_a_sharpened_render` is what holds that to bit
equality, and it is the test that fails first if the arithmetic drifts.

## L1 — `image_prep.rs` (24)

| Test | Asserts |
|---|---|
| `downscale_preserves_aspect_ratio` | 4 source shapes land on the right preview size |
| `downscale_leaves_small_images_untouched` | no resampling at or below the cap |
| `downscale_preserves_a_flat_colour_exactly` | u16 byte round trip is clean |
| `downscale_keeps_channels_distinct` | no channel swap during resize |
| `downscale_agrees_across_channel_counts` | `U16x3` and `U16x4` resample to the same pixels |
| `histogram_counts_every_pixel_once` | bins sum to the pixel count |
| `histogram_bins_by_the_high_byte` | u16 → bin mapping is `>> 8` |
| `histogram_channels_are_not_crossed` | R/G/B stay separate |
| `histograms_do_not_depend_on_the_channel_count` | the stride is the only thing separating the bins |
| `histograms_describe_the_full_resolution_image` | counted before downscaling, not after |
| `preview_uses_the_shared_linearisation` | the direct path uploads the shared conversion's bytes |
| `preparing_a_three_channel_source_matches_its_rgba_equivalent` | both branches, resampled and not |
| `the_preview_is_resampled_in_linear_light` | a checkerboard averages to linear 0.5, not 0.21 |
| `the_preview_is_the_top_level_of_the_pyramid` | the payload's preview is what the frontend falls back to between tiles |
| `prepare_reports_both_resolutions` | full-res and preview sizes both reported |
| `overview_covers_the_whole_frame_within_its_own_cap` | the histogram's texture is the whole frame, at its own size |
| `the_overview_is_resampled_in_linear_light` | the overview averages light too, so the bins do |
| `a_small_source_gets_an_overview_identical_to_its_preview` | nothing inside the cap is resampled twice |
| `payload_header_carries_the_full_resolution_of_a_downscaled_preview` | the anchor for pixel-denominated sliders |
| `payload_round_trips_through_the_frontend_layout` | bytes parse as `openImage.ts` reads them |
| `payload_header_carries_preview_not_source_dimensions` | header sizes the pixel view correctly |

## L1 — `pyramid.rs` (18)

The tiles the preview streams. Level 0 has to be the bytes the export uploads,
and every reduced level has to average light, or zooming changes the image.

| Test | Asserts |
|---|---|
| `an_image_within_the_preview_edge_is_a_single_level` | nothing to reduce at or below the cap |
| `each_level_halves_the_one_below_until_the_preview_edge_is_reached` | a 61 MP frame gets four levels |
| `odd_sizes_round_up_so_every_source_pixel_has_a_parent` | halving rounds up, never drops a column |
| `only_the_top_level_fits_the_preview_edge` | the pyramid stops at the first level within the cap |
| `edge_tiles_are_cut_short_by_what_is_left_of_the_level` | right and bottom tiles carry only real pixels |
| `a_tile_off_the_level_is_refused` | out-of-range columns, rows and levels are errors |
| `full_resolution_tiles_are_the_source_linearised_exactly` | level 0 stitched == the shared linearisation |
| `a_source_without_alpha_tiles_fully_opaque` | RGB and RGBA sources tile to the same bytes |
| `a_baked_source_tiles_as_its_converted_pixels` | a LUT profile is converted per tile |
| `a_flat_image_stays_flat_at_every_level` | no level shifts a flat colour |
| `reduced_levels_average_light_not_its_encoding` | a checkerboard reduces to linear 0.5 |
| `a_level_is_the_same_whichever_order_the_levels_are_built_in` | levels are independent of each other |
| `reduced_levels_are_only_built_when_asked_for` | building one level builds no other |
| `prefetching_builds_the_levels_either_side` | the neighbours of a requested level get built |
| `prefetching_skips_full_resolution_and_the_ends_of_the_pyramid` | nothing to build past either end |
| `a_prefetch_outlived_by_its_image_does_nothing` | a closed image's queued build never starts |
| `the_tile_payload_carries_each_tile_behind_its_header` | the batch layout `tileSource.ts` parses |
| `every_pyramid_is_told_apart_from_the_last` | a tile request names its image, so a stale one is refused |

## L1 — `source_space.rs` (23)

What a file says its own numbers mean, and how that reaches the GPU. Nothing
downstream can tell a mistake here from a deliberate look: a transposed matrix, a
channel routed through the wrong curve, or a column written four bytes early all
produce a plausible image in the wrong colours, and every slider the user then
moves is compensating for it.

The matrices are checked against profiles lcms2 builds itself rather than against
our own restatement of the same numbers, which is why a Rec.2020 profile
decomposing to the identity is a real assertion and not a tautology.

| Test | Asserts |
|---|---|
| `an_untagged_file_is_srgb_and_converts_on_the_gpu` | no profile means sRGB, matrix path |
| `a_tagged_srgb_profile_agrees_with_the_untagged_constant` | a real sRGB profile decomposes to `SRGB_TO_WORKING` |
| `a_rec2020_profile_decomposes_to_the_identity` | which a transpose cannot survive |
| `an_unreadable_profile_falls_back_to_srgb` | garbage bytes are no profile, so they are an untagged file |
| `a_greyscale_profile_undoes_its_own_curve_on_every_channel` | one TRC for three channels, identity matrix, matrix path kept |
| `a_profile_the_decoder_already_rendered_falls_back_to_srgb` | Lab/CMYK/XYZ describe numbers we no longer hold |
| `a_profile_missing_a_colorant_takes_the_cpu_path` | all six tags or none |
| `a_conversion_that_cannot_be_built_fails_the_open` | a profile lcms2 refuses is not silently ignored — see below |
| `a_lut_profile_bakes_to_the_same_place_the_matrix_path_reaches` | 6 probes, lcms2's grid against three curves and a 3x3 |
| `baking_carries_alpha_through_untouched` | alpha survives the three-wide reweaving |
| `baking_agrees_between_three_and_four_channel_sources` | both shapes reach the same colours |
| `a_lut_profile_opens_through_the_baked_path` | the whole open, flag and matrix included |
| `the_baked_path_hands_the_upload_an_identity_curve` | no second decode over already-linear data |
| `a_baked_source_sets_the_payload_flag_and_sends_the_identity` | the flag and matrix the frontend reads |
| `each_channel_goes_through_its_own_curve` | three gammas, routed and applied in order |
| `alpha_is_not_sent_through_a_curve` | alpha carries no transfer encoding |
| `the_resampled_route_routes_each_channel_through_its_own_curve` | the linear u16 path has its own indexing |
| `a_downscaled_overview_carries_the_per_channel_curves` | and `prepare_image` reaches it |
| `the_uniform_pads_each_matrix_column_to_sixteen_bytes` | a `mat3x3<f32>` is three vec4 columns |
| `the_payload_carries_the_matrix_and_the_baked_flag` | at the offsets `openImage.ts` reads |

### Which path a profile takes

`bake` hands lcms2 `PixelFormat::RGB_16`, so it can only convert a profile whose
own colour space is RGB. The decoder has already produced three or four channels
of u16 whatever the profile says, so what decides the path is whether the profile
still describes that buffer:

| Profile | Path | Why |
|---|---|---|
| none, or unreadable bytes | sRGB on the GPU | what an untagged file means in practice |
| RGB, curves + colorants | its own matrix, on the GPU | the darkroom block keeps the file's channels |
| RGB, A2B0 only | baked on the CPU | no separable channels to preserve |
| greyscale | its own TRC, identity matrix | one channel replicated across three; a neutral stays neutral |
| Lab, CMYK, XYZ | sRGB on the GPU | the decoder rendered these to RGB on its own terms |

Greyscale keeps the matrix path on purpose: a scanned black-and-white negative is
the case, and the per-channel black, white and gamma still address the three
channels the decoder made.

### The LUT profile fixture

`tests/fixtures/srgb-lut.icc` is the input `bake` exists for: an RGB profile
carrying its conversion as an A2B0 pipeline rather than as curves and colorants,
which is the shape a scanner's profile has.

lcms2's safe API cannot build one from stages — a `Pipeline` is only ever created
empty and concatenated — so `examples/make_lut_profile.rs` borrows one. It builds
a device link for sRGB → Lab, which holds exactly such a pipeline, and rehouses
that pipeline in an ordinary input profile. Regenerate with:

```
cargo run --example make_lut_profile
```

It describes sRGB on purpose. That is what lets the baked conversion be checked
against the matrix path rather than against a restatement of itself, and the two
agree to 5e-4 of full scale — a tenth of an 8-bit code value, which is the grid's
own interpolation error. A coarse grid keeps the committed file at 30 KB; the
default precalculation writes 216 KB for no accuracy a test can use.

That refusal is also the guard on the pixel type. lcms2 checks a transform's
element type against the format's bytes per pixel with a plain `assert_eq!`, so a
one-wide element against `RGB_16` panicked rather than returning — inside a Tauri
command, on opening an ordinary file. A clean `Err` is what says the check was
passed and the conversion was genuinely attempted.

## L1 — `raw.rs` (12)

Sensor data, which reaches `SourceSpace` by a different route: no transfer curve
to undo, and primaries from the camera's own calibration rather than a profile.
Three decisions nothing downstream could catch.

The colour matrix is checked against the published XYZ → sRGB matrix rather than
against our own numbers. Feed a camera whose primaries *are* sRGB and the
conversion has to land on `SRGB_TO_WORKING`, the constant an untagged file
already uses — a composition built the wrong way round cannot arrive there.
Orientation is checked exhaustively because all eight cases produce an image and
seven of them produce the wrong one.

| Test | Asserts |
|---|---|
| `srgb_camera_lands_on_the_untagged_matrix` | invert, compose and normalise, against a matrix from the spec |
| `neutral_stays_neutral` | rows sum to one, so a balanced grey stays grey |
| `a_matrix_of_the_wrong_width_is_refused` | a four-colour matrix is an error, not a silent crop |
| `white_balance_is_scaled_by_its_largest_coefficient` | nothing is pushed past unity, so nothing clips into u16 |
| `white_balance_preserves_a_neutral` | the scaling is uniform, which is what lets the matrix normalise separately |
| `missing_coefficients_fall_back_to_unity` | NaN, zero, negative and infinite all mean "no balance recorded" |
| `normal_is_not_copied_or_changed` | the common case allocates nothing |
| `flips_keep_the_dimensions` | the three in-place cases, by corner |
| `quarter_turns_swap_the_dimensions` | the four transposing cases, by corner |
| `every_orientation_keeps_every_pixel` | no pixel invented, dropped or doubled |
| `rotating_four_quarter_turns_returns_the_original` | which a near-miss in the index maths cannot survive |
| `raw_extensions_are_recognised_whatever_their_case` | the dispatch that decides which decoder a file gets |

### What is not covered here

There is no end-to-end decode. A camera file is 20–60 MB and does not belong in
the repository, and rawler's DNG writer is involved enough that a synthesised
fixture is its own piece of work. So the three pure decisions are tested and the
decode around them is not — the gap worth closing first if sensor support grows.

## L2 — `colorimetry.rs` (8)

The colorimetric constants and the Planckian locus in `calcParams.wgsl`, measured
against published values. Everything else in the white-balance suite is
directional — warmer looks warmer, the R/B ratio rises — which passes for a
transposed matrix or a mistyped coefficient as long as the ordering survives.
These pin the numbers.

Both entry points are probes appended to the shipped shader source, so what is
measured is the constant the pipeline compiles, not a copy kept in the test.

| Test | Asserts |
|---|---|
| `the_bradford_matrices_are_a_true_inverse_pair` | `M_BRADFORD_INV · M_BRADFORD` = I |
| `the_xyz_and_rgb_matrices_are_a_true_inverse_pair` | `M_XYZ_TO_RGB · M_RGB_TO_XYZ` = I |
| `the_rgb_to_xyz_matrix_maps_white_to_d65` | primaries sum to D65, identifying the matrix as a D65 space's |
| `the_luminance_coefficients_are_bt2020` | the matrix Y row and the `LR`/`LG`/`LB` constants both match BT.2020 |
| `the_source_matrix_is_composed_before_white_balance` | `wb_matrix` = WB · source, not the reverse |
| `the_locus_matches_published_blackbody_chromaticities` | 15 temperatures vs tabulated CIE 1931 values |
| `the_locus_is_continuous_across_its_branch_boundaries` | no step at 2222 K or 4000 K |
| `the_locus_runs_monotonically_across_the_slider_range` | 133 points over 2700–12000 K |

### The branch these tests were written to find

`cct_to_xyz` had two `y(x)` branches where Kim et al. publish three, so the
1667–2222 K polynomial was being used all the way to 4000 K. That put a 3.4e-3
step in y at exactly 4000 K and up to 3.5e-3 of chromaticity error across
3000–4000 K — the tungsten range. Above 4500 K the shader was already accurate
to ~1e-4, which is why every directional test passed.

The tolerances are split at 4000 K because the fit itself is: ~1.6e-4 above,
~5e-4 toward the bottom of the slider. `y` is only checked for monotonicity
above 2500 K — the locus genuinely peaks near 2250 K and falls away on both
sides, so `x` carries the ordering below that.

## L2 — `develop_gpu.rs` (17)

`calcParams.wgsl`, `develop.wgsl` and `histogram.wgsl` on real hardware. The chain
as a whole; for one probe per slider see [`sliders_gpu.rs`](#l2--sliders_gpurs-37).

| Test | Asserts |
|---|---|
| `default_sliders_build_identity_matrices` | all three matrices are identity to 1e-5 |
| `percentage_sliders_are_scaled_to_the_shader_range` | every −100..100 slider's scale factor; exposure passes through |
| `gamma_is_clamped_away_from_zero` | gamma 0 or negative is clamped |
| `inversion_flips_the_darkroom_matrix_slope` | slope sign flips, offset moves to output white |
| `extreme_sliders_never_produce_non_finite_params` | 9 rail positions, no inf/NaN |
| `default_sliders_are_an_identity_transform` | image in == image out |
| `primaries_stay_on_their_own_channels` | no transposed matrix |
| `exposure_scales_linear_light` | +1 EV doubles linear light |
| `per_channel_gamma_lifts_midtones` | gamma hits only its own channel |
| `inversion_is_its_own_inverse` | invert twice returns the original (16-bit) |
| `inversion_reverses_the_tone_order` | black ↔ white |
| `a_ramp_stays_ordered_under_every_tone_control` | 10 slider sets incl. both rails, ramp stays monotonic |
| `black_and_white_points_remap_the_endpoints` | new white point clips to white |
| `every_pixel_casts_exactly_one_vote` | histogram weight is exactly pixels × 256 |
| `vote_totals_hold_under_extreme_sliders` | same, at the rails |
| `a_solid_patch_lands_on_its_own_bin` | flat colour occupies ≤ 2 adjacent bins |
| `histogram_agrees_with_the_developed_image` | the two copies of the chain still match |

## L2 — `sliders_gpu.rs` (41)

One synthetic probe per control the UI exposes. `develop_gpu.rs` covers the chain
as a whole; this covers each slider on its own — correct direction, correct
magnitude where a closed-form reference exists, and no leakage into channels or
tonal regions the slider should not touch.

Three controls have exact CPU oracles rather than direction checks, because their
maths is a closed form: contrast (`contrast_reference`), brightness (a uniform
code-value shift), and tint (green scaled, then renormalised).

| Test | Asserts |
|---|---|
| `each_black_point_lifts_only_its_own_channel` | linear-light remap, R/G/B isolated |
| `a_black_point_clips_everything_below_it` | tones under the black point crush |
| `each_white_point_pulls_only_its_own_channel` | input at the white point clips, others held |
| `the_output_range_maps_the_endpoints_exactly` | black→64, white→192, mid interpolates linearly |
| `raising_the_output_black_lifts_the_shadows_off_zero` | pure black lands on the output black |
| `the_output_range_holds_its_endpoints_under_gamma` | gamma 2.2 does not drag output black 80 / white 200 |
| `the_output_range_holds_its_endpoints_when_inverted_under_gamma` | same, inverted: white→80, black→200 |
| `each_gamma_lifts_only_its_own_channel` | 3 channels vs an f64 reference |
| `gamma_below_one_darkens_the_midtones` | exponent sign is not inverted |
| `raising_the_temperature_warms_the_image` | 3200 K → B>G>R, 8800 K → R>G>B |
| `the_red_to_blue_ratio_rises_with_temperature` | monotonic across 5 temperatures |
| `tint_moves_green_against_the_slider` | green scales by 2^(-tint/100), then renormalises |
| `white_balance_does_not_change_the_exposure` | 90 in-gamut temp × tint combos hold luminance within 4% |
| `white_balance_still_shifts_the_colour` | the normalisation has not swallowed the control |
| `the_coldest_temperatures_desaturate_rather_than_clipping` | pins the 2200 K gamut limit, now paid in saturation |
| `exposure_is_symmetric_in_stops` | ±1 and ±2 EV are exact powers of two |
| `contrast_matches_the_reference_curve` | 17 tones × 6 settings vs an f64 oracle |
| `contrast_holds_mid_grey_and_both_endpoints` | pivot holds, 0 stays 0, 255 stays 255 |
| `contrast_keeps_the_tone_order_at_every_setting` | 7 settings incl. both rails, no fold |
| `negative_contrast_inverts_positive_contrast` | +50 then −50 returns the original tone |
| `positive_contrast_widens_the_tonal_spread` | shadows down, highlights up |
| `negative_contrast_narrows_the_tonal_spread` | spread shrinks |
| `brightness_shifts_every_tone_by_the_same_amount` | exact code-value shift, 4 settings, stays neutral |
| `brightness_keeps_the_tone_order_at_every_setting` | both rails |
| `highlights_act_only_above_mid_grey` | mask opens at sRGB 128, both directions |
| `shadows_act_only_below_mid_grey` | mask closes at sRGB 128, favours deeper tones |
| `whites_act_only_in_the_top_quarter` | opens at sRGB 191, and *ramps* rather than steps |
| `blacks_act_only_in_the_bottom_quarter` | closes at sRGB 64, reaches pure black |
| `the_four_masks_cover_four_different_ranges` | all 6 pairs differ; the two sides never cross |
| `the_region_controls_keep_the_tone_order_in_combination` | **13 cases** incl. all four at both rails |
| `saturation_at_the_bottom_rail_gives_neutral_grey` | −100 lands on the input's CIE Y exactly |
| `positive_saturation_widens_the_channel_spread` | both directions move correctly |
| `vibrance_boosts_muted_colours_more_than_vivid_ones` | the property that distinguishes it from saturation |
| `vibrance_measures_saturation_in_the_destination_space` | a closed form that rules out the working-space measurement |
| `vibrance_leaves_a_neutral_neutral` | no cast at either rail |
| `vibrance_does_not_resurrect_crushed_shadows` | 3 crushing setups, no black pixel lit, still active higher up |
| `hue_rotates_about_the_neutral_axis` | a full turn is I, a neutral does not move, ±120 go the right way |
| `hue_rotation_is_reversible_and_wraps` | ±60 differ, ±180 agree |
| `hue_leaves_a_neutral_neutral` | 5 angles, no cast |
| `every_slider_changes_the_rendered_image` | **all 24 lanes**, moved alone, alter the output |
| `the_neutral_axis_stays_neutral` | 12 non-channel sliders introduce no tint |

### Why the tone block is monotone by construction

The two tone-order defects this file used to document are fixed, not tolerated.
Both came from the same place: a control whose strength could outrun the shape of
its own curve.

**Contrast** was `mix(rgb, sigmoid(rgb), contrast)`. A negative factor
extrapolates *away* from the sigmoid, and since `f'(x) = (1−t) + t·S'(x)` with
`S'(0.5) = 2.5`, the slope vanished at `t = −2/3` — below contrast −67 the curve
ran backwards. It is now a power curve on each side of a mid-grey pivot,
`k = 3^contrast`, which is monotone for every `k > 0`, fixes 0 / pivot / 1, and
makes the negative branch the exact inverse of the positive one.

**The four region controls** are `x + A·mask(x)`. `smoothstep(0, W, x)` peaks at
slope `1.5/W`, so a single control folds once `A > W/1.5`, and because the masks
overlap their slopes add. `calcParams.wgsl` now budgets the amplitudes jointly —
`2.25·0.20 + 6.0·0.07 = 0.87 < 1` at the binding point, the peak of the black
mask — so no combination of the four can fold. The UI sliders are −100..100.

Both also moved into a **perceptually encoded** domain along with brightness, which
is where Lightroom's Basic panel works. That is what puts the mask boundaries at
sRGB 64 / 128 / 191 instead of 137 / 188 / 225, and what makes contrast pivot on
real mid-grey instead of on a bright highlight.

## L2 — `export_gpu.rs` (25)

Export mechanics: the parts that break on unusual dimensions, not unusual sliders.

| Test | Asserts |
|---|---|
| `chunk_size_does_not_change_the_result` | 7 chunk sizes, identical output |
| `no_seam_appears_at_a_chunk_boundary` | no step at a chunk edge |
| `images_shorter_than_one_chunk_render_whole` | single-chunk path |
| `awkward_widths_survive_row_padding` | 10 widths through 256-byte row padding |
| `awkward_dimensions_survive_at_16_bit` | same at 16 bytes per texel |
| `a_single_pixel_renders` | 1×1 image |
| `eight_and_sixteen_bit_paths_agree` | 16-bit >> 8 == 8-bit |
| `sixteen_bit_output_carries_more_levels_than_eight` | 16-bit actually resolves more |
| `a_highlight_ramp_keeps_every_sixteen_bit_code` | 256 consecutive codes below white survive |
| `progress_increases_and_finishes_at_one` | monotonic, ends at 1.0 |
| `a_ragged_final_chunk_still_reports_completion` | short trailing chunk |
| `malformed_input_is_rejected_before_touching_the_gpu` | 6 bad inputs, including a bad channel count |
| `a_three_channel_source_renders_exactly_like_its_rgba_equivalent` | byte-identical at 8 and 16 bits |
| `chunking_a_three_channel_source_leaves_no_seam` | 5 chunk sizes at a stride of three |
| `a_neutral_render_round_trips_through_the_working_space` | 8 probes, sRGB → Rec.2020 → sRGB unchanged |
| `the_export_applies_the_source_matrix_it_is_given` | a file already in the working space takes the output transform alone |
| `the_gamut_compression_keeps_the_luminance_a_clamp_would_lose` | the one thing exactly true of the compression — see below |
| `colour_the_destination_can_hold_passes_through_unchanged` | in-gamut colour is not quietly desaturated |
| `colour_brighter_than_white_resolves_to_white` | a colour pushed past white renders pure white |
| `highlight_rolloff_keeps_gradation_above_white` | with the roll-off, +1 to +4 EV stay distinct and below white, +5 EV is white |
| `highlight_rolloff_leaves_tones_below_its_shoulder_alone` | both modes leave darker tones byte-identical |
| `highlight_rolloff_modes_differ_on_saturated_colour` | the mode setting reaches the shader |
| `the_working_space_destination_only_applies_its_curve` | Rec.2020 out converts no primaries, so the BT.2020 OETF alone |
| `the_destination_primaries_move_colour_and_leave_neutrals` | sRGB vs Display P3: same white, different gamut |
| `each_destination_encodes_with_its_own_curve` | Adobe RGB's pure power vs sRGB's linear toe, in the shadows |

### Why luminance is the assertion

The gamut compression pulls a colour the destination cannot hold towards the
neutral of its own brightness. Hue is kept because the colour moves along a
straight line to that neutral, and luminance is kept *exactly*: the neutral has
the colour's own luminance, and luminance is linear, so every point on the line
between them has it too. For any luminance within 0..1 that holds for any amount
of compression, which makes it
the one property worth asserting rather than approximating.

A per-channel clamp keeps neither. It drops whichever channel left the range and
leaves the others, which darkens the colour and slides its hue — for a Rec.2020
red into sRGB, by about 0.05 of luminance against a tolerance of 0.005. The test
checks that gap too, so it cannot pass by accident on a probe that was in gamut
all along.

One case is not a gamut question: a pixel whose luminance is itself above white
has no in-gamut neutral to aim at, and resolves to white. That is a deliberate
change from the clamp, which left such a pixel its most-clipped channel and so
tinted blown highlights.

## L2 — `encoding.rs` (15)

| Test | Asserts |
|---|---|
| `png_round_trips_exactly` | lossless |
| `png_compression_levels_all_produce_valid_files` | 4 levels, pixels unchanged |
| `tiff_round_trips_exactly_at_eight_bits` | lossless |
| `tiff_round_trips_exactly_at_sixteen_bits` | every 16-bit level preserved |
| `webp_lossless_round_trips_exactly` | lossless |
| `bw_tiff_writes_a_single_channel_from_red` | greyscale L8, not RGB |
| `bw_tiff_at_sixteen_bits_keeps_the_full_range` | greyscale L16 |
| `jpeg_stays_close_to_the_source` | quality 95 mean error < 4 |
| `higher_jpeg_quality_produces_a_larger_file` | quality slider reaches the encoder |
| `the_webp_lossless_flag_reaches_the_encoder` | lossless exact, lossy not |
| `an_unsupported_tiff_bit_depth_is_reported` | 12-bit errors by name |
| `every_format_carries_the_profile_of_its_space` | 4 formats × 4 spaces, each read back through `SourceSpace` |
| `a_sixteen_bit_tiff_is_tagged_and_still_exact` | the strip-wise path tags without disturbing a sample |
| `a_greyscale_tiff_is_tagged` | the single-channel paths at 8 and 16 bits |
| `rewrapping_webp_for_its_profile_preserves_the_image` | the extended container — see below |

### Why the TIFF tag is read by hand

Neither library here can vouch for the profile in a TIFF. `image`'s decoder
looks it up as `Tag::Unknown(34675)`, while the `tiff` crate parses that same id
into its named `IccProfile` variant — so the two never meet, and a profile that
is present reads as absent. Taking either at its word would have meant either a
test that could never pass or, worse, one that passed by agreeing with the same
mistake twice.

Whether the bytes are right is a question about the file, so the test parses the
first IFD itself: tag 34675, type 7 (UNDEFINED), and the profile at the offset
the entry gives. That also pins the tag *type*, which a reader is entitled to
check and which writing `[u8]` through the `tiff` crate would have silently made
BYTE instead.

### Why WebP is rewrapped

libwebp writes the simple RIFF container: one `VP8 ` or `VP8L` chunk and nothing
else, with nowhere to put a profile. Carrying one means the extended format, so
the encoded bitstream is copied into a new container behind a `VP8X` header
declaring the canvas and an `ICCP` chunk holding the profile. The bitstream
itself is never touched, and the test checks both halves of that claim: the
header is there, and the pixels still decode to what went in.

## L1 — `tests/unit/` (152, TypeScript)

| Test | Asserts |
|---|---|
| `calcMatrixHelpers` × 5 | identity, diagonals, column-major order, non-commutativity, shape |
| `openImage` × 9 | header, pixel slicing, the overview region, R/G/B histogram order, no region overlap, odd sizes |
| `sharpnessPage` × 11 | the sharpness controls' ranges, defaults, steps and history wiring |
| `openImage` × 3 more | the source matrix's float order, the baked flag, the pixels after the 64-byte header |
| `wgslInclude` × 10 | the directive's grammar, chunk flattening, and every shipped shader resolving |
| `outputSpace` × 12 | the destination matrices derived on the frontend: identity on the working space, inverses, white, gamut containment, the curve table |
| `openImage` × 1 more | the pyramid structure is fetched once the image is open |
| `viewport` × 20 | the affine maps, fitting and zooming about the cursor, level choice, what a frame covers, 1:1 landing on texel centres |
| `tileSource` × 7 | the tile payload as `pyramid.rs` writes it, which tiles a region needs, assembly into one texture, the cache, stale replies |
| `adjustmentsClipboard` × 11, `settingsPersistence` × 14, `shortcuts` × 20 | the panels' own state |

## L3 — `tests/browser/` (40, real WebGPU)

Drives the frontend's own pipeline modules, including the full six-pass chain. A
failure here that passes in L2 is a TypeScript wiring bug.

| Test | Asserts |
|---|---|
| `exposes an adapter and a device` | WebGPU reachable |
| `builds every stage of the chain against the shipped WGSL` | all six stages compile, layouts valid |
| `declares buffer sizes matching the WGSL structs` | 128, 256, 16 and 80 bytes |
| `accepts a params bind group built the way renderer.ts builds it` | bind group is legal |
| `leaves the image untouched with the default sliders` | identity, as in L2 |
| `keeps the primaries on their own channels` | no channel crossing |
| `scales linear light by one stop of exposure` | matches the CPU reference |
| `inverts, reversing the tone order` | black ↔ white |
| `applies gamma to one channel only` | per-channel gamma |
| `packs the sliders in the order the shader reads them` | two sliders at once, right order |
| `carries the sharpness sliders without disturbing the ones before them` | lane isolation, as in L1 |
| `renders identically at any render scale` | the view moves nothing while the mask is off |
| `renders the same whether or not the view was ever written` | the 1.0 default is real |
| `overshoots on both sides of an edge` | the halo, through the TypeScript wiring |
| `leaves the image alone when the amount is zero` | bit-exact no-op |
| `fades the mask out as the render scale falls` | the zoomed-out preview stops sharpening |
| `moves the image with texture and with clarity` | both bands reach the pixels, and differ |
| `bins the same values the preview renders` | 4 saturated probes: the histogram and the preview share one output transform |
| `returns the image it was given, pixel for pixel, at identity` | a payload framed as the backend frames one, through the whole chain |
| `holds the edges in place` | corners and both ends of every row |
| `agrees with the renderer about when the blur can be skipped` | `sharpnessIsVisible` tracks the shader's fade |
| `leaves clear exactly the pixels outside the clip` | the preview draws nothing past the image |
| `copies texels exactly when render pixels land on their centres` | the view sample stage at 1:1 is a copy |
| `repeats the border past the edges of the source` | the blurs see the image's own edge, not black |
