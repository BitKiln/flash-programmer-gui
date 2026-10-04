//! An in-memory OpenOCD, answering the commands this backend sends.
//!
//! Its job is to make the session logic testable: geometry building, alignment
//! rules, the file dance around `flash write_image`, verification, and the
//! local-only interlock all run against this in CI with no OpenOCD installed
//! and no board attached.
//!
//! It models NOR flash honestly — an erased cell reads `0xFF`, and a write can
//! only clear bits — because a simulator that lets a write flip a bit back to
//! one would pass code that cannot work on a real part.

use std::collections::HashMap;

use flash_core::error::FlashError;

use crate::tcl::TclLink;

/// Sector layout of the simulated part: an STM32F4-like bank.
const SECTORS: &[(u32, u32)] = &[
    (0x0000_0000, 16 * 1024),
    (0x0000_4000, 16 * 1024),
    (0x0000_8000, 16 * 1024),
    (0x0000_C000, 16 * 1024),
    (0x0001_0000, 64 * 1024),
];

pub const FLASH_BASE: u32 = 0x0800_0000;
pub const RAM_BASE: u32 = 0x2000_0000;
pub const RAM_SIZE: u32 = 32 * 1024;

/// A simulated OpenOCD process with a simulated target behind it.
pub struct FakeOpenOcd {
    flash: Vec<u8>,
    ram: Vec<u8>,
    halted: bool,
    /// Whether the simulated OpenOCD is on this machine, and can therefore
    /// open a file this process wrote.
    local: bool,
    /// True once `flash probe` has been run, since an unprobed bank reports no
    /// size — which is exactly the state a real one starts in.
    probed: bool,
    /// Every command received, so a test can assert on what was sent rather
    /// than only on the outcome.
    pub commands: Vec<String>,
    /// Replies to return instead of simulating, keyed by the exact command.
    /// For making OpenOCD fail in the way a test needs.
    pub canned: HashMap<String, String>,
    /// When set, no target is marked current, as in a configuration that names
    /// none.
    pub no_current_target: bool,
    /// When set, `flash list` reports nothing.
    pub no_flash_banks: bool,
    pub reset_count: usize,
}

impl Default for FakeOpenOcd {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeOpenOcd {
    pub fn new() -> Self {
        let size: u32 = SECTORS.iter().map(|(_, size)| size).sum();
        Self {
            flash: vec![0xFF; size as usize],
            ram: vec![0; RAM_SIZE as usize],
            halted: false,
            local: true,
            probed: false,
            commands: Vec::new(),
            canned: HashMap::new(),
            no_current_target: false,
            no_flash_banks: false,
            reset_count: 0,
        }
    }

    /// A simulated OpenOCD on another machine: its file system is not ours.
    pub fn remote() -> Self {
        Self {
            local: false,
            ..Self::new()
        }
    }

    pub fn flash_bytes(&self) -> &[u8] {
        &self.flash
    }

    /// Whether the simulated core is halted, which a real one must be before
    /// flash can be written.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    fn targets_reply(&self) -> String {
        let state = if self.halted { "halted" } else { "running" };
        let marker = if self.no_current_target { ' ' } else { '*' };
        format!(
            "    TargetName         Type       Endian TapName            State\n\
             --  ------------------ ---------- ------ ------------------ ------------\n\
             \x200{marker} sim.cpu            cortex_m   little sim.cpu            {state}"
        )
    }

    fn flash_info_reply(&self) -> String {
        if !self.probed {
            return "Error: flash bank 0 not probed".to_string();
        }
        let size: u32 = SECTORS.iter().map(|(_, size)| size).sum();
        let mut reply =
            format!("#0 : sim at {FLASH_BASE:#010x}, size {size:#010x}, buswidth 4, chipwidth 0");
        for (index, (offset, sector_size)) in SECTORS.iter().enumerate() {
            reply.push_str(&format!(
                "\n\t# {index}: {offset:#010x} ({sector_size:#x} {}kB) not protected",
                sector_size / 1024
            ));
        }
        reply
    }

    /// Index of the flash byte at `address`, or `None` when it is outside.
    fn flash_index(&self, address: u32) -> Option<usize> {
        let end = FLASH_BASE + self.flash.len() as u32;
        (address >= FLASH_BASE && address < end).then(|| (address - FLASH_BASE) as usize)
    }

    fn ram_index(&self, address: u32) -> Option<usize> {
        let end = RAM_BASE + self.ram.len() as u32;
        (address >= RAM_BASE && address < end).then(|| (address - RAM_BASE) as usize)
    }

    fn read_range(&self, address: u32, length: u32) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(length as usize);
        for offset in 0..length {
            let at = address.checked_add(offset)?;
            let byte = match (self.flash_index(at), self.ram_index(at)) {
                (Some(i), _) => self.flash[i],
                (_, Some(i)) => self.ram[i],
                _ => return None,
            };
            out.push(byte);
        }
        Some(out)
    }

    /// Erases whole sectors covering `address..address + length`, the way a
    /// real driver does: a partial range still takes the sector with it.
    fn erase_covering(&mut self, address: u32, length: u32) -> Result<(), String> {
        if self.flash_index(address).is_none() {
            return Err(format!(
                "Error: address {address:#x} is not in any flash bank"
            ));
        }
        let end = address.saturating_add(length);
        for (offset, size) in SECTORS {
            let sector_start = FLASH_BASE + offset;
            let sector_end = sector_start + size;
            if address < sector_end && end > sector_start {
                let from = (sector_start - FLASH_BASE) as usize;
                let to = from + *size as usize;
                self.flash[from..to].fill(0xFF);
            }
        }
        Ok(())
    }

    /// Writes with NOR rules: a bit can only go from one to zero.
    fn write_flash(&mut self, address: u32, data: &[u8]) -> Result<(), String> {
        for (offset, byte) in data.iter().enumerate() {
            let at = address.saturating_add(offset as u32);
            let Some(index) = self.flash_index(at) else {
                return Err(format!("Error: address {at:#x} is not in any flash bank"));
            };
            let current = self.flash[index];
            if current & byte != *byte {
                return Err(format!(
                    "Error: failed writing {byte:#04x} over un-erased {current:#04x} at {at:#x}"
                ));
            }
            self.flash[index] = current & byte;
        }
        Ok(())
    }

    fn write_ram(&mut self, address: u32, data: &[u8]) -> Result<(), String> {
        for (offset, byte) in data.iter().enumerate() {
            let at = address.saturating_add(offset as u32);
            let Some(index) = self.ram_index(at) else {
                return Err(format!("Error: unable to write memory at {at:#x}"));
            };
            self.ram[index] = *byte;
        }
        Ok(())
    }

    /// Strips the braces and quotes TCL arguments arrive in.
    fn unquote(argument: &str) -> String {
        argument
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim_matches('"')
            .to_string()
    }

    fn number(token: &str) -> Option<u32> {
        let text = token.trim();
        if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            text.parse().ok()
        }
    }

    fn answer(&mut self, command: &str) -> String {
        if let Some(canned) = self.canned.get(command) {
            return canned.clone();
        }

        // `capture {cmd}` returns what cmd printed to the console.
        let command = command.trim();
        let inner = command
            .strip_prefix("capture ")
            .map(Self::unquote)
            .unwrap_or_else(|| command.to_string());
        let fields: Vec<&str> = inner.split_whitespace().collect();

        match fields.as_slice() {
            ["targets"] => self.targets_reply(),
            ["halt"] => {
                self.halted = true;
                String::new()
            }
            ["reset", rest @ ..] => {
                self.reset_count += 1;
                self.halted = rest.first().is_some_and(|r| *r == "halt");
                String::new()
            }
            ["flash", "list"] => {
                if self.no_flash_banks {
                    String::new()
                } else {
                    let size = if self.probed {
                        SECTORS.iter().map(|(_, s)| s).sum::<u32>()
                    } else {
                        0
                    };
                    format!(
                        "{{name sim.flash driver sim base {} size {} bus_width 4 chip_width 0}}",
                        FLASH_BASE, size
                    )
                }
            }
            ["flash", "probe", _] => {
                self.probed = true;
                format!("flash 'sim' found at {FLASH_BASE:#010x}")
            }
            ["flash", "info", _] => self.flash_info_reply(),
            ["flash", "erase_sector", _, _, _] => {
                self.flash.fill(0xFF);
                "erased sectors 0 through 4".to_string()
            }
            ["flash", "erase_address", rest @ ..] => {
                // The leading "unlock" is optional, so read from the end.
                let numbers: Vec<u32> = rest.iter().filter_map(|t| Self::number(t)).collect();
                match numbers.as_slice() {
                    [address, length] => match self.erase_covering(*address, *length) {
                        Ok(()) => format!("erased address {address:#010x} (length {length})"),
                        Err(e) => e,
                    },
                    _ => "Error: wrong # args for flash erase_address".to_string(),
                }
            }
            ["flash", "write_image", rest @ ..] => {
                if !self.local {
                    return "Error: couldn't open file".to_string();
                }
                let erase = rest.contains(&"erase");
                let arguments: Vec<&&str> = rest
                    .iter()
                    .filter(|a| **a != "erase" && **a != "unlock")
                    .collect();
                let Some(path) = arguments.first().map(|a| Self::unquote(a)) else {
                    return "Error: wrong # args for flash write_image".to_string();
                };
                let Some(address) = arguments.get(1).and_then(|a| Self::number(a)) else {
                    return "Error: wrong # args for flash write_image".to_string();
                };
                let Ok(data) = std::fs::read(&path) else {
                    return format!("Error: couldn't open {path}");
                };
                if erase {
                    if let Err(e) = self.erase_covering(address, data.len() as u32) {
                        return e;
                    }
                }
                match self.write_flash(address, &data) {
                    Ok(()) => format!("wrote {} bytes from file {path}", data.len()),
                    Err(e) => e,
                }
            }
            ["load_image", rest @ ..] => {
                if !self.local {
                    return "Error: couldn't open file".to_string();
                }
                let Some(path) = rest.first().map(|a| Self::unquote(a)) else {
                    return "Error: wrong # args for load_image".to_string();
                };
                let Some(address) = rest.get(1).and_then(|a| Self::number(a)) else {
                    return "Error: wrong # args for load_image".to_string();
                };
                let Ok(data) = std::fs::read(&path) else {
                    return format!("Error: couldn't open {path}");
                };
                match self.write_ram(address, &data) {
                    Ok(()) => format!("downloaded {} bytes", data.len()),
                    Err(e) => e,
                }
            }
            ["dump_image", rest @ ..] => {
                if !self.local {
                    return "Error: couldn't open file".to_string();
                }
                let Some(path) = rest.first().map(|a| Self::unquote(a)) else {
                    return "Error: wrong # args for dump_image".to_string();
                };
                let Some(address) = rest.get(1).and_then(|a| Self::number(a)) else {
                    return "Error: wrong # args for dump_image".to_string();
                };
                let Some(length) = rest.get(2).and_then(|a| Self::number(a)) else {
                    return "Error: wrong # args for dump_image".to_string();
                };
                let Some(bytes) = self.read_range(address, length) else {
                    return format!("Error: unable to read memory at {address:#x}");
                };
                match std::fs::write(&path, &bytes) {
                    Ok(()) => format!("dumped {length} bytes in 0.01s"),
                    Err(e) => format!("Error: couldn't open {path}: {e}"),
                }
            }
            ["mdb", rest @ ..] => {
                let Some(address) = rest.first().and_then(|a| Self::number(a)) else {
                    return "Error: wrong # args for mdb".to_string();
                };
                let length = rest.get(1).and_then(|a| Self::number(a)).unwrap_or(1);
                let Some(bytes) = self.read_range(address, length) else {
                    return format!("Error: unable to read memory at {address:#x}");
                };
                let mut reply = String::new();
                for (row, chunk) in bytes.chunks(16).enumerate() {
                    let row_address = address + (row * 16) as u32;
                    reply.push_str(&format!("{row_address:#010x}: "));
                    reply.push_str(
                        &chunk
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    reply.push('\n');
                }
                reply
            }
            ["mwb", address, value] => {
                let Some(address) = Self::number(address) else {
                    return "Error: wrong # args for mwb".to_string();
                };
                let Some(value) = Self::number(value) else {
                    return "Error: wrong # args for mwb".to_string();
                };
                match self.write_ram(address, &[value as u8]) {
                    Ok(()) => String::new(),
                    Err(e) => e,
                }
            }
            [target, "cget", option] if target.contains('.') => match *option {
                "-work-area-phys" => format!("{RAM_BASE:#x}"),
                "-work-area-size" => format!("{RAM_SIZE:#x}"),
                other => format!("Error: unknown option {other}"),
            },
            _ => format!("invalid command name \"{}\"", fields.first().unwrap_or(&"")),
        }
    }
}

impl TclLink for FakeOpenOcd {
    fn command(&mut self, command: &str) -> Result<String, FlashError> {
        self.commands.push(command.to_string());
        Ok(self.answer(command))
    }

    fn is_local(&self) -> bool {
        self.local
    }

    fn endpoint(&self) -> String {
        if self.local {
            "127.0.0.1:6666 (simulated)".to_string()
        } else {
            "openocd.example:6666 (simulated)".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unprobed_bank_reports_no_size() {
        let mut fake = FakeOpenOcd::new();
        assert!(fake.command("flash list").unwrap().contains("size 0"));
        let _ = fake.command("flash probe 0").unwrap();
        assert!(!fake.command("flash list").unwrap().contains("size 0"));
    }

    #[test]
    fn a_write_over_un_erased_flash_fails_the_way_nor_does() {
        let mut fake = FakeOpenOcd::new();
        let path = std::env::temp_dir().join("flashgui_fake_nor_test.bin");
        std::fs::write(&path, [0x00]).unwrap();
        let command = format!("flash write_image {} 0x08000000 bin", path.display());

        // 0x00 over erased 0xFF only clears bits, so it is allowed.
        assert!(!fake.command(&command).unwrap().starts_with("Error"));
        // 0xFF over 0x00 would set bits, which needs an erase first.
        std::fs::write(&path, [0xFF]).unwrap();
        assert!(fake.command(&command).unwrap().contains("un-erased"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_remote_openocd_cannot_open_our_files() {
        let mut fake = FakeOpenOcd::remote();
        assert!(fake
            .command("flash write_image erase {/tmp/x.bin} 0x08000000 bin")
            .unwrap()
            .contains("couldn't open"));
    }

    #[test]
    fn halting_and_resetting_move_the_simulated_core() {
        let mut fake = FakeOpenOcd::new();
        assert!(!fake.is_halted());
        let _ = fake.command("halt").unwrap();
        assert!(fake.is_halted());
        let _ = fake.command("reset run").unwrap();
        assert!(!fake.is_halted());
        let _ = fake.command("reset halt").unwrap();
        assert!(fake.is_halted());
        assert_eq!(fake.reset_count, 2);
    }
}
