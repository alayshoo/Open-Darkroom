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

Nothing is `#[ignore]`d. 115 Rust tests, 10 TypeScript, 10 in-browser.

---

## L1 — `shader_contracts.rs` (10)

Reads source files as text. Catches a field added in one language and not the others.

| Test | Asserts |
|---|---|
| `sliders_struct_is_all_f32_and_matches_the_declared_size` | 24 f32 fields = 96 bytes |
| `rust_slider_packing_matches_the_wgsl_struct` | `sliders_to_bytes` order == WGSL order |
| `typescript_slider_packing_matches_the_wgsl_struct` | `slidersToArray` order == WGSL order |
| `typescript_declares_the_same_buffer_sizes_as_rust` | TS buffer-size constants == Rust |
| `rust_default_sliders_match_the_frontend_defaults` | `SlidersPayload::default()` == `defaultSlidersRGB` |
| `params_struct_is_identical_across_every_shader` | `Params` same in all 3 WGSL files |
| `params_struct_matches_the_declared_size` | `Params` = 240 bytes |
| `params_field_offsets_match_the_readback_accessors` | test harness offsets still valid |
| `develop_and_histogram_apply_the_same_chain` | the duplicated develop chain has not drifted |
| `develop_and_histogram_share_the_same_tone_maths` | so has the duplicated tone maths it calls |

## L1 — `color.rs` (6)

| Test | Asserts |
|---|---|
| `lut_spans_the_domain_and_is_monotonic` | 65536 entries, 0→0, 1→1, non-decreasing |
| `lut_tracks_the_reference_curve` | matches an independent f64 sRGB curve |
| `f16_storage_survives_a_round_trip_to_8_bit` | f16 costs < 0.5 of 255 |
| `linearize_produces_eight_bytes_per_pixel` | output sizing |
| `linearize_maps_each_channel_through_the_lut` | no channel crossing; alpha not gamma-decoded |
| `linearize_is_order_independent` | rayon parallelism has no data race |

## L1 — `image_prep.rs` (13)

| Test | Asserts |
|---|---|
| `downscale_preserves_aspect_ratio` | 4 source shapes land on the right preview size |
| `downscale_leaves_small_images_untouched` | no resampling at or below the cap |
| `downscale_preserves_a_flat_colour_exactly` | u16 byte round trip is clean |
| `downscale_keeps_channels_distinct` | no channel swap during resize |
| `histogram_counts_every_pixel_once` | bins sum to the pixel count |
| `histogram_bins_by_the_high_byte` | u16 → bin mapping is `>> 8` |
| `histogram_channels_are_not_crossed` | R/G/B stay separate |
| `histograms_describe_the_full_resolution_image` | counted before downscaling, not after |
| `preview_uses_the_same_linearisation_as_export` | preview and export share one code path |
| `the_preview_is_resampled_in_linear_light` | a checkerboard averages to linear 0.5, not 0.21 |
| `prepare_reports_both_resolutions` | full-res and preview sizes both reported |
| `payload_round_trips_through_the_frontend_layout` | bytes parse as `openImage.ts` reads them |
| `payload_header_carries_preview_not_source_dimensions` | header sizes the pixel view correctly |

## L2 — `colorimetry.rs` (7)

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
| `the_rgb_to_xyz_matrix_maps_white_to_d65` | primaries sum to D65, identifying the matrix as sRGB's |
| `the_luminance_coefficients_are_bt709` | the matrix Y row and the `LR`/`LG`/`LB` constants both match BT.709 |
| `the_locus_matches_published_blackbody_chromaticities` | 15 temperatures vs tabulated CIE 1931 values |
| `the_locus_is_continuous_across_its_branch_boundaries` | no step at 2222 K or 4000 K |
| `the_locus_runs_monotonically_across_the_slider_range` | 133 points over 2200–8800 K |

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

## L2 — `sliders_gpu.rs` (40)

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
| `the_coldest_temperatures_clip_red_out_of_gamut` | pins the 2200 K gamut limit |
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
| `saturation_at_the_bottom_rail_gives_neutral_grey` | −100 lands on BT.709 luma exactly |
| `positive_saturation_widens_the_channel_spread` | both directions move correctly |
| `vibrance_boosts_muted_colours_more_than_vivid_ones` | the property that distinguishes it from saturation |
| `vibrance_leaves_a_neutral_neutral` | no cast at either rail |
| `vibrance_does_not_resurrect_crushed_shadows` | 3 crushing setups, no black pixel lit, still active higher up |
| `a_hundred_and_twenty_degrees_permutes_the_primaries` | exact channel rotation about (1,1,1) |
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

## L2 — `export_gpu.rs` (11)

Export mechanics: the parts that break on unusual dimensions, not unusual sliders.

| Test | Asserts |
|---|---|
| `chunk_size_does_not_change_the_result` | 7 chunk sizes, identical output |
| `no_seam_appears_at_a_chunk_boundary` | no step at a chunk edge |
| `images_shorter_than_one_chunk_render_whole` | single-chunk path |
| `awkward_widths_survive_row_padding` | 10 widths through 256-byte row padding |
| `awkward_dimensions_survive_at_16_bit` | same at 8 bytes per texel |
| `a_single_pixel_renders` | 1×1 image |
| `eight_and_sixteen_bit_paths_agree` | 16-bit >> 8 == 8-bit |
| `sixteen_bit_output_carries_more_levels_than_eight` | 16-bit actually resolves more |
| `progress_increases_and_finishes_at_one` | monotonic, ends at 1.0 |
| `a_ragged_final_chunk_still_reports_completion` | short trailing chunk |
| `malformed_input_is_rejected_before_touching_the_gpu` | 4 bad inputs return errors |

## L2 — `encoding.rs` (11)

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

## L1 — `tests/unit/` (10, TypeScript)

| Test | Asserts |
|---|---|
| `multiplyMat4` × 5 | identity, diagonals, column-major order, non-commutativity, shape |
| `openImage` × 5 | header, pixel slicing, R/G/B histogram order, no region overlap, odd sizes |

## L3 — `tests/browser/` (10, real WebGPU)

Drives the frontend's own pipeline modules. A failure here that passes in L2 is a
TypeScript wiring bug.

| Test | Asserts |
|---|---|
| `exposes an adapter and a device` | WebGPU reachable |
| `builds both pipelines against the shipped WGSL` | shaders compile, layouts valid |
| `declares buffer sizes matching the WGSL structs` | 96 and 240 bytes |
| `accepts a params bind group built the way renderer.ts builds it` | bind group is legal |
| `leaves the image untouched with the default sliders` | identity, as in L2 |
| `keeps the primaries on their own channels` | no channel crossing |
| `scales linear light by one stop of exposure` | matches the CPU reference |
| `inverts, reversing the tone order` | black ↔ white |
| `applies gamma to one channel only` | per-channel gamma |
| `packs the sliders in the order the shader reads them` | two sliders at once, right order |
