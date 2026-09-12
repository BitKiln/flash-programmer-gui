//! Serial-number programming: stamping a unique value into each board as it is
//! programmed.
//!
//! The value is written as its own memory segment, so it lives wherever the
//! product puts it (a dedicated sector, a slot at the end of flash, an OTP-like
//! page) rather than being patched into the firmware image.

use std::sync::atomic::{AtomicU64, Ordering};

use firmware_parser::MemorySegment;
use serde::{Deserialize, Serialize};

use crate::error::FlashError;
use crate::traits::FlashSession;

/// How the serial value is laid out in flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialEncoding {
    /// The rendered text as ASCII bytes, padded to `width` with `pad`.
    Ascii,
    /// The counter value as a little-endian u32.
    U32Le,
    /// The counter value as a big-endian u32.
    U32Be,
    /// The counter value as a little-endian u64.
    U64Le,
}

/// Where each board's serial value comes from and how it is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialConfig {
    /// Flash address the value is written to.
    pub address: u32,
    /// Template for the rendered text. `{n}` is replaced by the counter value;
    /// `{n:06}` pads it to six digits with leading zeros.
    pub format: String,
    /// Counter value used for the first board.
    pub start: u64,
    /// Added to the counter after each board.
    pub step: u64,
    pub encoding: SerialEncoding,
    /// Total bytes written. ASCII values are padded to this width; numeric
    /// encodings ignore it in favour of their natural size.
    pub width: usize,
    /// Byte used to pad an ASCII value out to `width`.
    pub pad: u8,
    /// Read the value back after writing it.
    pub verify: bool,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            address: 0,
            format: "{n}".to_string(),
            start: 1,
            step: 1,
            encoding: SerialEncoding::Ascii,
            width: 16,
            pad: 0xFF,
            verify: true,
        }
    }
}

impl SerialConfig {
    /// Bytes this configuration writes per board.
    pub fn byte_width(&self) -> usize {
        match self.encoding {
            SerialEncoding::Ascii => self.width,
            SerialEncoding::U32Le | SerialEncoding::U32Be => 4,
            SerialEncoding::U64Le => 8,
        }
    }

    /// Renders the text for one counter value.
    ///
    /// `{n}` interpolates the counter; `{n:0<width>}` pads it with zeros. An
    /// unrecognised placeholder is left as written, so a malformed template is
    /// visible in the log rather than silently dropping the serial.
    pub fn render(&self, counter: u64) -> String {
        let mut out = String::with_capacity(self.format.len() + 8);
        let bytes: Vec<char> = self.format.chars().collect();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] != '{' {
                out.push(bytes[i]);
                i += 1;
                continue;
            }
            let Some(close) = bytes[i..].iter().position(|c| *c == '}') else {
                out.push(bytes[i]);
                i += 1;
                continue;
            };
            let spec: String = bytes[i + 1..i + close].iter().collect();
            match parse_placeholder(&spec) {
                Some(pad_to) => {
                    let digits = counter.to_string();
                    for _ in digits.len()..pad_to {
                        out.push('0');
                    }
                    out.push_str(&digits);
                    i += close + 1;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        }
        out
    }

    /// Encodes one board's serial into the bytes written to flash.
    pub fn encode(&self, counter: u64, rendered: &str) -> Result<Vec<u8>, FlashError> {
        match self.encoding {
            SerialEncoding::Ascii => {
                let text = rendered.as_bytes();
                if text.len() > self.width {
                    return Err(FlashError::ProgramError(format!(
                        "Serial '{}' is {} bytes, which does not fit the {}-byte field",
                        rendered,
                        text.len(),
                        self.width
                    )));
                }
                let mut bytes = text.to_vec();
                bytes.resize(self.width, self.pad);
                Ok(bytes)
            }
            SerialEncoding::U32Le | SerialEncoding::U32Be => {
                let value = u32::try_from(counter).map_err(|_| {
                    FlashError::ProgramError(format!(
                        "Serial counter {} does not fit a 32-bit field",
                        counter
                    ))
                })?;
                Ok(match self.encoding {
                    SerialEncoding::U32Be => value.to_be_bytes().to_vec(),
                    _ => value.to_le_bytes().to_vec(),
                })
            }
            SerialEncoding::U64Le => Ok(counter.to_le_bytes().to_vec()),
        }
    }
}

/// Reads a `{n}` or `{n:0<width>}` placeholder, returning the zero-pad width.
fn parse_placeholder(spec: &str) -> Option<usize> {
    if spec == "n" {
        return Some(0);
    }
    let rest = spec.strip_prefix("n:")?;
    let digits = rest.strip_prefix('0').unwrap_or(rest);
    digits.parse::<usize>().ok()
}

/// Hands out consecutive serial values, one per board.
///
/// The counter is shared and atomic because the batch runner may drive it from
/// a worker thread while the UI reads the last value issued.
#[derive(Debug)]
pub struct SerialAllocator {
    config: SerialConfig,
    next: AtomicU64,
}

impl SerialAllocator {
    pub fn new(config: SerialConfig) -> Self {
        let start = config.start;
        Self {
            config,
            next: AtomicU64::new(start),
        }
    }

    #[inline]
    pub fn config(&self) -> &SerialConfig {
        &self.config
    }

    /// The counter the next board will be given.
    pub fn peek(&self) -> u64 {
        self.next.load(Ordering::SeqCst)
    }

    /// Takes the next counter value, advancing by `step`.
    pub fn take(&self) -> u64 {
        self.next.fetch_add(self.config.step.max(1), Ordering::SeqCst)
    }
}

/// Writes one serial value to the target and returns the rendered text.
///
/// The field is erased first unless the backend erases what it programs, so a
/// board that already carries a serial is re-stamped rather than bit-anded with
/// the old value.
pub fn program_serial(
    session: &mut dyn FlashSession,
    config: &SerialConfig,
    counter: u64,
) -> Result<String, FlashError> {
    let rendered = config.render(counter);
    let bytes = config.encode(counter, &rendered)?;

    if let Some(target) = session.target_info() {
        let end = config.address.saturating_add(bytes.len() as u32);
        if config.address < target.flash_base || end > target.flash_end() {
            return Err(FlashError::AddressOutOfBounds {
                address: config.address,
                base: target.flash_base,
                size: target.flash_size,
            });
        }
    }

    let segments = vec![MemorySegment::new(config.address, bytes.clone())];

    if !session.program_erases_target() {
        session.erase_range(config.address, bytes.len() as u32, None)?;
    }
    // Verification and reset belong to the caller's flash lifecycle, not to
    // this one small write.
    let options = crate::types::ProgramOptions {
        verify_after: false,
        reset_after: false,
        chip_erase: false,
        chunk_size: 256,
    };
    session.program(&segments, &options, None)?;

    if config.verify {
        let read_back = session.read_memory(config.address, bytes.len() as u32)?;
        if read_back != bytes {
            let at = read_back
                .iter()
                .zip(bytes.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            return Err(FlashError::VerificationMismatch {
                address: config.address + at as u32,
                expected: bytes[at],
                actual: read_back.get(at).copied().unwrap_or(0),
            });
        }
    }

    Ok(rendered)
}
