// src-tauri/src/jxl_encoding.rs
//
// JPEG XL output through libjxl. Lossless keeps the samples exactly, in the
// space the profile names; lossy encodes in XYB, where the distance setting is
// a perceptual target rather than a quantiser step.

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;
use std::ptr;

use jxl_sys::*;

pub struct JxlOptions {
    pub lossless: bool,
    /// JPEG-style quality, 1–100, mapped to a butteraugli distance by libjxl.
    pub quality: u8,
    /// Encoder effort, 1 (fastest) – 9 (smallest).
    pub effort: u8,
    /// Bits per sample stored in the file, 8 – 16. The input is 8-bit at 8 and
    /// 16-bit above, which libjxl rescales to this depth.
    pub bit_depth: u8,
    pub greyscale: bool,
}

/// Owns the encoder and its thread pool, so every early return releases both.
struct Encoder {
    enc: *mut JxlEncoder,
    runner: *mut c_void,
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: both pointers came from their Create calls and are released
        // exactly once, the encoder first since it holds the runner.
        unsafe {
            if !self.enc.is_null() {
                JxlEncoderDestroy(self.enc);
            }
            if !self.runner.is_null() {
                JxlThreadParallelRunnerDestroy(self.runner);
            }
        }
    }
}

impl Encoder {
    fn check(&self, status: JxlEncoderStatus, what: &str) -> Result<(), String> {
        match status {
            JxlEncoderStatus::JXL_ENC_SUCCESS => Ok(()),
            _ => {
                // SAFETY: `enc` is valid for the lifetime of `self`.
                let error = unsafe { JxlEncoderGetError(self.enc) };
                Err(format!("JPEG XL {what} failed: {error:?}"))
            }
        }
    }
}

/// The distance libjxl would pick for a JPEG-style quality: 90 is 1.0, the
/// visually lossless point, and lower distances keep more fine texture.
pub fn distance_from_quality(quality: u8) -> f32 {
    // SAFETY: a pure function of its argument.
    unsafe { JxlEncoderDistanceFromQuality(f32::from(quality.clamp(1, 100))) }
}

/// Encode RGB samples (8-bit, or 16-bit little-endian for depths above 8) to a
/// JPEG XL file. libjxl reads the pixels straight from `rgb_bytes` and writes
/// the file as it goes, so neither side is copied whole.
pub fn encode_jxl(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    options: &JxlOptions,
    icc: &[u8],
    path: &Path,
) -> Result<(), String> {
    let bytes_per_sample = match options.bit_depth {
        8 => 1,
        9..=16 => 2,
        other => return Err(format!("Unsupported JPEG XL bit depth: {other}")),
    };

    let expected = width as usize * height as usize * 3 * bytes_per_sample;
    if rgb_bytes.len() != expected {
        return Err(format!(
            "JPEG XL input is {} bytes, expected {expected}",
            rgb_bytes.len()
        ));
    }

    let mut samples = rgb_bytes;
    if options.greyscale {
        keep_first_channel(&mut samples, bytes_per_sample);
    }
    let channels: u32 = if options.greyscale { 1 } else { 3 };

    let pixel_bytes = channels as usize * bytes_per_sample;
    let input = Input {
        data: &samples,
        width: width as usize,
        height: height as usize,
        pixel_bytes,
        row_bytes: width as usize * pixel_bytes,
        format: JxlPixelFormat {
            num_channels: channels,
            data_type: if bytes_per_sample == 2 {
                JxlDataType::JXL_TYPE_UINT16
            } else {
                JxlDataType::JXL_TYPE_UINT8
            },
            endianness: JxlEndianness::JXL_LITTLE_ENDIAN,
            align: 0,
        },
    };
    let mut output = Output {
        file: BufWriter::with_capacity(OUTPUT_CHUNK, File::create(path).map_err(|e| e.to_string())?),
        buffer: Vec::new(),
        error: None,
    };

    // SAFETY: plain constructors; null results are checked before use.
    let encoder = unsafe {
        Encoder {
            enc: JxlEncoderCreate(ptr::null()),
            runner: JxlThreadParallelRunnerCreate(
                ptr::null(),
                JxlThreadParallelRunnerDefaultNumWorkerThreads(),
            ),
        }
    };
    if encoder.enc.is_null() || encoder.runner.is_null() {
        return Err("Failed to create the JPEG XL encoder".to_string());
    }
    let enc = encoder.enc;

    // SAFETY: `enc` and `runner` are valid until `encoder` drops, which happens
    // before `input` and `output`, the two callback targets, go out of scope.
    let status = unsafe {
        encoder.check(
            JxlEncoderSetParallelRunner(enc, Some(JxlThreadParallelRunner), encoder.runner),
            "thread setup",
        )?;
        encoder.check(
            JxlEncoderSetOutputProcessor(enc, output.processor()),
            "output setup",
        )?;

        let mut info = std::mem::MaybeUninit::<JxlBasicInfo>::uninit();
        JxlEncoderInitBasicInfo(info.as_mut_ptr());
        let mut info = info.assume_init();
        info.xsize = width;
        info.ysize = height;
        info.bits_per_sample = u32::from(options.bit_depth);
        info.exponent_bits_per_sample = 0;
        info.num_color_channels = channels;
        // Lossless has to stay in the tagged space; lossy works in XYB and
        // carries the profile along to convert back on decode.
        info.uses_original_profile = i32::from(options.lossless);
        encoder.check(JxlEncoderSetBasicInfo(enc, &info), "image header")?;

        encoder.check(
            JxlEncoderSetICCProfile(enc, icc.as_ptr(), icc.len()),
            "colour profile",
        )?;

        let frame = JxlEncoderFrameSettingsCreate(enc, ptr::null());
        if frame.is_null() {
            return Err("Failed to create JPEG XL frame settings".to_string());
        }
        encoder.check(
            JxlEncoderFrameSettingsSetOption(
                frame,
                JxlEncoderFrameSettingId::JXL_ENC_FRAME_SETTING_EFFORT,
                i64::from(options.effort.clamp(1, 9)),
            ),
            "effort setting",
        )?;
        if options.lossless {
            encoder.check(JxlEncoderSetFrameLossless(frame, 1), "lossless setting")?;
        } else {
            encoder.check(
                JxlEncoderSetFrameDistance(frame, distance_from_quality(options.quality)),
                "quality setting",
            )?;
        }

        // With an output processor set, the frame is encoded during this call.
        let added = JxlEncoderAddChunkedFrame(frame, 1, input.source());
        if added == JxlEncoderStatus::JXL_ENC_SUCCESS {
            JxlEncoderCloseInput(enc);
            JxlEncoderFlushInput(enc)
        } else {
            added
        }
    };
    // A write that failed inside a callback is the real cause of any error.
    if let Some(error) = output.error.take() {
        return Err(format!("Failed to write the JPEG XL file: {error}"));
    }
    encoder.check(status, "encode")?;
    drop(encoder);

    output.file.flush().map_err(|e| e.to_string())
}

/// Pack the first sample of every RGB pixel at the front of the buffer, in
/// place. In BW mode R=G=B, so this is the grey channel.
fn keep_first_channel(samples: &mut Vec<u8>, bytes_per_sample: usize) {
    let stride = 3 * bytes_per_sample;
    let pixels = samples.len() / stride;
    for i in 0..pixels {
        for k in 0..bytes_per_sample {
            samples[i * bytes_per_sample + k] = samples[i * stride + k];
        }
    }
    samples.truncate(pixels * bytes_per_sample);
}

// ── Input ─────────────────────────────────────────────────────────────────────

/// The pixels libjxl pulls rectangles from, in place.
struct Input<'a> {
    data: &'a [u8],
    width: usize,
    height: usize,
    pixel_bytes: usize,
    row_bytes: usize,
    format: JxlPixelFormat,
}

impl Input<'_> {
    fn source(&self) -> JxlChunkedFrameInputSource {
        JxlChunkedFrameInputSource {
            opaque: self as *const Self as *mut c_void,
            get_color_channels_pixel_format: Some(input_format),
            get_color_channel_data_at: Some(input_data_at),
            get_extra_channel_pixel_format: Some(no_extra_channel_format),
            get_extra_channel_data_at: Some(no_extra_channel_data_at),
            release_buffer: Some(release_input),
        }
    }
}

unsafe extern "C" fn input_format(opaque: *mut c_void, format: *mut JxlPixelFormat) {
    // SAFETY: `opaque` is the `Input` behind `source()`, alive while libjxl runs.
    let input = unsafe { &*(opaque as *const Input) };
    unsafe { *format = input.format };
}

/// Called from libjxl's worker threads, so it only reads.
unsafe extern "C" fn input_data_at(
    opaque: *mut c_void,
    xpos: usize,
    ypos: usize,
    xsize: usize,
    ysize: usize,
    row_offset: *mut usize,
) -> *const c_void {
    // SAFETY: as in `input_format`.
    let input = unsafe { &*(opaque as *const Input) };
    if xpos + xsize > input.width || ypos + ysize > input.height {
        return ptr::null();
    }
    match input.data.get(ypos * input.row_bytes + xpos * input.pixel_bytes..) {
        Some(rest) => {
            unsafe { *row_offset = input.row_bytes };
            rest.as_ptr().cast()
        }
        None => ptr::null(),
    }
}

unsafe extern "C" fn no_extra_channel_format(_: *mut c_void, _: usize, _: *mut JxlPixelFormat) {}

unsafe extern "C" fn no_extra_channel_data_at(
    _: *mut c_void,
    _: usize,
    _: usize,
    _: usize,
    _: usize,
    _: usize,
    _: *mut usize,
) -> *const c_void {
    ptr::null()
}

/// The rectangles point into the caller's buffer, so there is nothing to free.
unsafe extern "C" fn release_input(_: *mut c_void, _: *const c_void) {}

// ── Output ────────────────────────────────────────────────────────────────────

/// Largest piece libjxl is handed to write into at a time.
const OUTPUT_CHUNK: usize = 1 << 20;

/// Where libjxl writes the file. Callbacks cannot return errors, so the first
/// one is kept here and stops the encoder at its next request for a buffer.
struct Output {
    file: BufWriter<File>,
    buffer: Vec<u8>,
    error: Option<io::Error>,
}

impl Output {
    fn processor(&mut self) -> JxlEncoderOutputProcessor {
        JxlEncoderOutputProcessor {
            opaque: self as *mut Self as *mut c_void,
            get_buffer: Some(output_buffer),
            release_buffer: Some(output_written),
            seek: Some(output_seek),
            set_finalized_position: Some(output_finalized),
        }
    }
}

unsafe extern "C" fn output_buffer(opaque: *mut c_void, size: *mut usize) -> *mut c_void {
    // SAFETY: `opaque` is the `Output` behind `processor()`. libjxl calls the
    // output callbacks one at a time and holds at most one buffer.
    let output = unsafe { &mut *(opaque as *mut Output) };
    if output.error.is_some() {
        unsafe { *size = 0 };
        return ptr::null_mut();
    }
    let len = unsafe { *size }.clamp(1, OUTPUT_CHUNK);
    output.buffer.resize(len, 0);
    unsafe { *size = len };
    output.buffer.as_mut_ptr().cast()
}

unsafe extern "C" fn output_written(opaque: *mut c_void, written_bytes: usize) {
    // SAFETY: as in `output_buffer`.
    let output = unsafe { &mut *(opaque as *mut Output) };
    if output.error.is_some() {
        return;
    }
    let result = match output.buffer.get(..written_bytes) {
        Some(bytes) => output.file.write_all(bytes),
        None => Err(io::Error::other("encoder wrote past its buffer")),
    };
    output.error = result.err();
}

unsafe extern "C" fn output_seek(opaque: *mut c_void, position: u64) {
    // SAFETY: as in `output_buffer`.
    let output = unsafe { &mut *(opaque as *mut Output) };
    if output.error.is_none() {
        output.error = output.file.seek(SeekFrom::Start(position)).err();
    }
}

unsafe extern "C" fn output_finalized(_: *mut c_void, _: u64) {}
