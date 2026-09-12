//! Reading a target's geometry out of what OpenOCD says about it.
//!
//! OpenOCD answers in two shapes and neither is a data format: `flash list`
//! returns a TCL list of dictionaries, and `flash info` prints a human-readable
//! table. Parsing both is the price of not asking the user to type a flash
//! layout in by hand.
//!
//! Everything here is a pure function over a string, which is how the parsing
//! is tested without an OpenOCD process.

use flash_core::types::SectorInfo;

/// One flash bank as `flash list` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bank {
    pub index: usize,
    pub name: String,
    pub driver: String,
    pub base: u32,
    /// Size in bytes. Zero until the bank has been probed, which is normal:
    /// most drivers only learn the size from the chip itself.
    pub size: u32,
}

/// Parses the reply to `flash list`.
///
/// The reply is a TCL list of dictionaries:
///
/// ```text
/// {name stm32f4x.flash driver stm32f2x base 134217728 size 0 bus_width 4 chip_width 0} {name ...}
/// ```
///
/// Values are decimal, and the key order is not guaranteed, so this reads by
/// key rather than by position.
pub fn parse_flash_list(reply: &str) -> Vec<Bank> {
    let mut banks = Vec::new();
    let mut rest = reply;

    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let body = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];

        let fields: Vec<&str> = body.split_whitespace().collect();
        let value = |key: &str| -> Option<&str> {
            fields
                .iter()
                .position(|f| *f == key)
                .and_then(|i| fields.get(i + 1))
                .copied()
        };

        let Some(base) = value("base").and_then(parse_number) else {
            continue;
        };
        banks.push(Bank {
            index: banks.len(),
            name: value("name").unwrap_or("flash").to_string(),
            driver: value("driver").unwrap_or("unknown").to_string(),
            base,
            size: value("size").and_then(parse_number).unwrap_or(0),
        });
    }

    banks
}

/// Parses a decimal or `0x`-prefixed number.
fn parse_number(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

/// Sectors and bank extent from the output of `flash info <bank>`.
///
/// The output looks like:
///
/// ```text
/// #0 : stm32f4x at 0x08000000, size 0x00080000, buswidth 4, chipwidth 0
///     # 0: 0x00000000 (0x4000 16kB) not protected
///     # 1: 0x00004000 (0x4000 16kB) not protected
/// ```
///
/// Sector addresses are **offsets from the bank base**, not absolute
/// addresses; getting that wrong puts every sector 0x08000000 too low. The
/// returned sectors are absolute, because that is what the rest of the program
/// deals in.
///
/// Returns `(base, size, sectors)`, with `size` zero when the header did not
/// say and no sectors were listed.
pub fn parse_flash_info(reply: &str) -> (u32, u32, Vec<SectorInfo>) {
    let mut base = 0u32;
    let mut size = 0u32;
    let mut sectors = Vec::new();

    for line in reply.lines() {
        let line = line.trim();

        // Header: "#0 : stm32f4x at 0x08000000, size 0x00080000, ..."
        if line.starts_with('#') && line.contains(" at ") {
            if let Some(at) = line.split(" at ").nth(1) {
                if let Some(value) = at.split(',').next() {
                    base = parse_number(value.trim()).unwrap_or(base);
                }
            }
            if let Some(after) = line.split("size ").nth(1) {
                if let Some(value) = after.split(',').next() {
                    size = parse_number(value.trim()).unwrap_or(size);
                }
            }
            continue;
        }

        // Sector: "# 0: 0x00000000 (0x4000 16kB) not protected"
        if let Some(rest) = line.strip_prefix('#') {
            let Some((index_text, tail)) = rest.split_once(':') else {
                continue;
            };
            let Ok(index) = index_text.trim().parse::<u32>() else {
                continue;
            };
            let tail = tail.trim();
            let Some(offset) = tail.split_whitespace().next().and_then(parse_number) else {
                continue;
            };
            let sector_size = tail
                .split_once('(')
                .and_then(|(_, inner)| inner.split_whitespace().next())
                .and_then(parse_number);
            let Some(sector_size) = sector_size else {
                continue;
            };
            sectors.push(SectorInfo {
                index,
                address: base.saturating_add(offset),
                size: sector_size,
            });
        }
    }

    // A bank whose header gave no size still has one: the sectors add up to it.
    if size == 0 && !sectors.is_empty() {
        size = sectors.iter().map(|s| s.size).sum();
    }

    (base, size, sectors)
}

/// Parses the reply to `targets`, returning the name of the current target.
///
/// ```text
///     TargetName         Type       Endian TapName            State
/// --  ------------------ ---------- ------ ------------------ ------------
///  0* stm32f4x.cpu       cortex_m   little stm32f4x.cpu       halted
/// ```
///
/// The asterisk marks the current target. A configuration with several targets
/// is normal on a multi-core part, and picking the wrong one would program the
/// wrong core's view of flash.
pub fn parse_current_target(reply: &str) -> Option<String> {
    for line in reply.lines() {
        let line = line.trim();
        // The marked row, e.g. " 0* stm32f4x.cpu  cortex_m ..."
        let Some((head, rest)) = line.split_once('*') else {
            continue;
        };
        if !head.trim().chars().all(|c| c.is_ascii_digit()) || head.trim().is_empty() {
            continue;
        }
        if let Some(name) = rest.split_whitespace().next() {
            return Some(name.to_string());
        }
    }
    None
}

/// Whether the `targets` output says the current target is halted.
///
/// Programming through OpenOCD needs a halted core; a running one fails part
/// way through with a message about the target not being halted.
pub fn current_target_is_halted(reply: &str) -> bool {
    for line in reply.lines() {
        let line = line.trim();
        if let Some((head, rest)) = line.split_once('*') {
            if head.trim().chars().all(|c| c.is_ascii_digit()) && !head.trim().is_empty() {
                return rest.to_lowercase().contains("halted");
            }
        }
    }
    false
}

/// Bytes from `mdb` output, which prints `address: bb bb bb bb`.
///
/// ```text
/// 0x20000000: de ad be ef
/// ```
///
/// Byte-wise rather than word-wise on purpose: a read of three bytes has no
/// answer in words, and assembling words into bytes would also have to guess
/// the target's endianness.
pub fn parse_mdb(reply: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in reply.lines() {
        let Some((_, values)) = line.trim().split_once(':') else {
            continue;
        };
        for token in values.split_whitespace() {
            match u8::from_str_radix(token, 16) {
                Ok(byte) => bytes.push(byte),
                // The trailing ASCII gutter some builds print is not data.
                Err(_) => break,
            }
        }
    }
    bytes
}

/// Words from `mdw` output, which prints `address: word word word`.
///
/// ```text
/// 0x20000000: deadbeef 00000000 cafebabe 12345678
/// ```
pub fn parse_mdw(reply: &str) -> Vec<u32> {
    let mut words = Vec::new();
    for line in reply.lines() {
        let Some((_, values)) = line.trim().split_once(':') else {
            continue;
        };
        for token in values.split_whitespace() {
            if let Ok(word) = u32::from_str_radix(token, 16) {
                words.push(word);
            }
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLASH_LIST: &str = "{name stm32f4x.flash driver stm32f2x base 134217728 size 0 \
         bus_width 4 chip_width 0} {name stm32f4x.otp driver stm32f2x_otp base 507510784 \
         size 528 bus_width 1 chip_width 1}";

    const FLASH_INFO: &str = "#0 : stm32f4x at 0x08000000, size 0x00080000, buswidth 4, chipwidth 0
	# 0: 0x00000000 (0x4000 16kB) not protected
	# 1: 0x00004000 (0x4000 16kB) not protected
	# 2: 0x00008000 (0x4000 16kB) not protected
	# 3: 0x0000c000 (0x4000 16kB) not protected
	# 4: 0x00010000 (0x10000 64kB) not protected";

    const TARGETS: &str = "    TargetName         Type       Endian TapName            State
--  ------------------ ---------- ------ ------------------ ------------
 0  stm32f4x.ap        mem_ap     little stm32f4x.cpu       unknown
 1* stm32f4x.cpu       cortex_m   little stm32f4x.cpu       halted";

    #[test]
    fn banks_are_read_by_key_rather_than_by_position() {
        let banks = parse_flash_list(FLASH_LIST);
        assert_eq!(banks.len(), 2);
        assert_eq!(banks[0].base, 0x0800_0000);
        assert_eq!(banks[0].driver, "stm32f2x");
        assert_eq!(banks[0].size, 0, "an unprobed bank reports no size");
        assert_eq!(banks[1].size, 528);
        assert_eq!(banks[1].index, 1);
    }

    #[test]
    fn a_reply_with_no_banks_is_an_empty_list() {
        assert!(parse_flash_list("").is_empty());
        assert!(parse_flash_list("Error: no flash bank found").is_empty());
    }

    #[test]
    fn sector_offsets_become_absolute_addresses() {
        let (base, size, sectors) = parse_flash_info(FLASH_INFO);
        assert_eq!(base, 0x0800_0000);
        assert_eq!(size, 0x0008_0000);
        assert_eq!(sectors.len(), 5);
        // The offsets in the output are relative to the bank base; treating
        // them as absolute would put every sector at address 0.
        assert_eq!(sectors[0].address, 0x0800_0000);
        assert_eq!(sectors[4].address, 0x0801_0000);
        assert_eq!(sectors[0].size, 0x4000);
        assert_eq!(sectors[4].size, 0x1_0000);
    }

    #[test]
    fn a_bank_with_no_size_in_its_header_is_measured_by_its_sectors() {
        let reply = "#0 : nrf5 at 0x00000000, buswidth 1, chipwidth 1
	# 0: 0x00000000 (0x1000 4kB) not protected
	# 1: 0x00001000 (0x1000 4kB) not protected";
        let (base, size, sectors) = parse_flash_info(reply);
        assert_eq!(base, 0);
        assert_eq!(size, 0x2000);
        assert_eq!(sectors.len(), 2);
    }

    #[test]
    fn flash_info_that_says_nothing_useful_yields_no_sectors() {
        let (_, size, sectors) = parse_flash_info("Error: flash bank 0 not probed");
        assert_eq!(size, 0);
        assert!(sectors.is_empty());
    }

    #[test]
    fn the_marked_row_is_the_current_target() {
        assert_eq!(
            parse_current_target(TARGETS).as_deref(),
            Some("stm32f4x.cpu"),
            "the asterisk marks it; the first row is a different target"
        );
        assert!(current_target_is_halted(TARGETS));
    }

    #[test]
    fn a_running_target_is_not_reported_as_halted() {
        let running = TARGETS.replace("halted", "running");
        assert!(!current_target_is_halted(&running));
    }

    #[test]
    fn no_marked_target_is_no_target_rather_than_the_first_one() {
        let unmarked = TARGETS.replace('*', " ");
        assert_eq!(parse_current_target(&unmarked), None);
        assert!(!current_target_is_halted(&unmarked));
    }

    #[test]
    fn memory_words_are_read_in_the_order_printed() {
        let reply = "0x20000000: deadbeef 00000000 cafebabe 12345678\n\
                     0x20000010: 00000001";
        assert_eq!(
            parse_mdw(reply),
            vec![0xdeadbeef, 0x0000_0000, 0xcafebabe, 0x1234_5678, 1]
        );
    }

    #[test]
    fn memory_bytes_are_read_byte_wise() {
        let reply = "0x20000000: de ad be ef
0x20000004: 00 ff";
        assert_eq!(parse_mdb(reply), vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0xff]);
    }

    #[test]
    fn a_number_may_be_decimal_or_hex() {
        assert_eq!(parse_number("134217728"), Some(0x0800_0000));
        assert_eq!(parse_number("0x08000000"), Some(0x0800_0000));
        assert_eq!(parse_number("nonsense"), None);
    }
}
