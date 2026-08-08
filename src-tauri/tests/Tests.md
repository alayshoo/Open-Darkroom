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

---

## L1 — `shader_contracts.rs` (9)

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

## L1 — `color.rs` (6)

| Test | Asserts |
|---|---|
| `lut_spans_the_domain_and_is_monotonic` | 65536 entries, 0→0, 1→1, non-decreasing |
| `lut_tracks_the_reference_curve` | matches an independent f64 sRGB curve |
| `f16_storage_survives_a_round_trip_to_8_bit` | f16 costs < 0.5 of 255 |
| `linearize_produces_eight_bytes_per_pixel` | output sizing |
| `linearize_maps_each_channel_through_the_lut` | no channel crossing; alpha not gamma-decoded |
| `linearize_is_order_independent` | rayon parallelism has no data race |

## L1 — `image_prep.rs` (12)

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
| `prepare_reports_both_resolutions` | full-res and preview sizes both reported |
| `payload_round_trips_through_the_frontend_layout` | bytes parse as `openImage.ts` reads them |
| `payload_header_carries_preview_not_source_dimensions` | header sizes the pixel view correctly |

## L2 — `develop_gpu.rs` (17)

`calcParams.wgsl`, `develop.wgsl` and `histogram.wgsl` on real hardware.

| Test | Asserts |
|---|---|
| `default_sliders_build_identity_matrices` | all three matrices are identity to 1e-5 |
| `percentage_sliders_are_scaled_to_unit_range` | contrast/vibrance ÷ 100, exposure passes through |
| `gamma_is_clamped_away_from_zero` | gamma 0 or negative is clamped |
| `inversion_flips_the_darkroom_matrix_slope` | slope sign flips, offset moves to output white |
| `extreme_sliders_never_produce_non_finite_params` | 9 rail positions, no inf/NaN |
| `default_sliders_are_an_identity_transform` | image in == image out |
| `primaries_stay_on_their_own_channels` | no transposed matrix |
| `exposure_scales_linear_light` | +1 EV doubles linear light |
| `per_channel_gamma_lifts_midtones` | gamma hits only its own channel |
| `inversion_is_its_own_inverse` | invert twice returns the original (16-bit) |
| `inversion_reverses_the_tone_order` | black ↔ white |
| `a_ramp_stays_ordered_under_every_tone_control` | 8 slider sets, ramp stays monotonic |
| `black_and_white_points_remap_the_endpoints` | new white point clips to white |
| `every_pixel_casts_exactly_one_vote` | histogram weight is exactly pixels × 256 |
| `vote_totals_hold_under_extreme_sliders` | same, at the rails |
| `a_solid_patch_lands_on_its_own_bin` | flat colour occupies ≤ 2 adjacent bins |
| `histogram_agrees_with_the_developed_image` | the two copies of the chain still match |

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
