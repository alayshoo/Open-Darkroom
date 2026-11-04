// src-tauri/src/jxl_encoding.rs
//
// JPEG XL output through libjxl. Lossless keeps the samples exactly, in the
// space the profile names; lossy encodes in XYB, where the distance setting is
// a perceptual target rather than a quantiser step.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::ptr;

use jxl_sys::*;

pub struct JxlOptions {
    pub lossless: bool,
    /// JPEG-style quality, 1–100, mapped to a butteraugli distance by libjxl.
    pub quality: u8,
    /// Encoder effort, 1 (fastest) – 9 (smallest).
    pub effort: u8,
    /// 8 or 16 bits per sample.
    pub bit_depth: u8,
    pub greyscale: bool,
}

/// Owns the encoder and its thread pool, so every early return releases both.
struct Encoder {
    enc: *mut JxlEncoder,
    runner: *mut std::ffi::c_void,
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

/// Encode RGB samples (8-bit, or 16-bit little-endian) to a JPEG XL file.
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
        16 => 2,
        other => return Err(format!("Unsupported JPEG XL bit depth: {other}")),
    };

    // In BW mode R=G=B, so the single channel is taken from red.
    let samples = if options.greyscale {
        let stride = 3 * bytes_per_sample;
        rgb_bytes
            .chunks_exact(stride)
            .flat_map(|px| px[..bytes_per_sample].iter().copied())
            .collect()
    } else {
        rgb_bytes
    };
    let channels: u32 = if options.greyscale { 1 } else { 3 };

    let expected = width as usize * height as usize * channels as usize * bytes_per_sample;
    if samples.len() != expected {
        return Err(format!(
            "JPEG XL input is {} bytes, expected {expected}",
            samples.len()
        ));
    }

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

    // SAFETY: `enc` and `runner` are valid until `encoder` drops at the end of
    // this function, and every pointer handed over outlives the call using it.
    unsafe {
        encoder.check(
            JxlEncoderSetParallelRunner(enc, Some(JxlThreadParallelRunner), encoder.runner),
            "thread setup",
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

        let format = JxlPixelFormat {
            num_channels: channels,
            data_type: if bytes_per_sample == 2 {
                JxlDataType::JXL_TYPE_UINT16
            } else {
                JxlDataType::JXL_TYPE_UINT8
            },
            endianness: JxlEndianness::JXL_LITTLE_ENDIAN,
            align: 0,
        };
        encoder.check(
            JxlEncoderAddImageFrame(frame, &format, samples.as_ptr().cast(), samples.len()),
            "frame",
        )?;
        JxlEncoderCloseInput(enc);
    }
    drop(samples);

    write_output(&encoder, path)
}

/// Drain the encoder to disk a buffer at a time, so the compressed file never
/// has to exist in memory alongside libjxl's own working set.
fn write_output(encoder: &Encoder, path: &Path) -> Result<(), String> {
    const CHUNK: usize = 4 << 20;

    let mut file = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    let mut buffer = vec![0u8; CHUNK];

    loop {
        let mut next = buffer.as_mut_ptr();
        let mut avail = buffer.len();
        // SAFETY: `next` and `avail` describe `buffer`, which libjxl only writes
        // within, advancing `next` by what it wrote.
        let status = unsafe { JxlEncoderProcessOutput(encoder.enc, &mut next, &mut avail) };
        let written = buffer.len() - avail;
        file.write_all(&buffer[..written]).map_err(|e| e.to_string())?;

        match status {
            JxlEncoderStatus::JXL_ENC_NEED_MORE_OUTPUT => continue,
            other => {
                encoder.check(other, "encode")?;
                break;
            }
        }
    }

    file.flush().map_err(|e| e.to_string())
}
