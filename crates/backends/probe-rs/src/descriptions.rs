//! Chip descriptions supplied at runtime.
//!
//! probe-rs is compiled with a fixed set of target definitions — 242 families
//! in 0.32, and **none of them Espressif**. Anything the registry does not know
//! is unreachable, which would mean a chip could only be supported by shipping
//! a new binary.
//!
//! Loading a probe-rs target YAML at runtime fixes that: a description from a
//! vendor, from `probe-rs target-gen`, or from `esp-rs/esp-flash-loader` for
//! the ESP JTAG path makes the chip usable without recompiling anything.

use std::path::{Path, PathBuf};

use flash_core::error::FlashError;
use probe_rs::config::{Registry, Target, TargetSelector};

/// Extra target descriptions to load on top of the built-in set.
#[derive(Debug, Clone, Default)]
pub struct TargetDescriptions {
    paths: Vec<PathBuf>,
}

impl TargetDescriptions {
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// A registry holding the built-in families plus every description given.
    ///
    /// A file that will not load is an error rather than a warning: someone who
    /// passed `--target-yaml` is relying on that chip being present, and
    /// silently falling back to the built-in set would produce a confusing
    /// "chip not found" much later.
    pub fn registry(&self) -> Result<Registry, FlashError> {
        let mut registry = Registry::from_builtin_families();
        for path in &self.paths {
            load_into(&mut registry, path)?;
        }
        Ok(registry)
    }

    /// Resolves `name` to a concrete target, so it can be attached even when
    /// probe-rs has no built-in description for it.
    ///
    /// Returns `None` when the name is not in the extra descriptions; the
    /// caller then uses the ordinary built-in lookup, which gives better error
    /// messages for a plain typo.
    pub fn resolve(&self, name: &str) -> Result<Option<TargetSelector>, FlashError> {
        if self.is_empty() {
            return Ok(None);
        }
        let registry = self.registry()?;
        Ok(lookup(&registry, name).map(TargetSelector::Specified))
    }
}

fn load_into(registry: &mut Registry, path: &Path) -> Result<(), FlashError> {
    let yaml = std::fs::read_to_string(path).map_err(|e| {
        FlashError::InvalidState(format!(
            "could not read the target description {}: {e}",
            path.display()
        ))
    })?;
    registry.add_target_family_from_yaml(&yaml).map_err(|e| {
        FlashError::TargetNotSupported(format!(
            "{} is not a valid probe-rs target description: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

/// Case-insensitive lookup, because part numbers get written every which way.
fn lookup(registry: &Registry, name: &str) -> Option<Target> {
    registry
        .get_target_by_name(name)
        .ok()
        .or_else(|| {
            let matches = registry.search_chips(name);
            let found = matches
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(name))
                .or_else(|| matches.first())?;
            registry.get_target_by_name(found).ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A real probe-rs target description, so these tests exercise the actual
    /// schema rather than a hand-written guess at it.
    ///
    /// Generating one at runtime is not an option: probe-rs's own families do
    /// not survive a serialise/deserialise round trip, because it writes
    /// `manufacturer.id` as a hex string and reads it back as a `u8`.
    const EXAMPLE_FAMILY: &str = include_str!("../tests/fixtures/example_family.yaml");

    /// Writes the fixture out under a chip name nothing else uses.
    fn write_family(chip_name: &str) -> tempfile::NamedTempFile {
        let yaml = EXAMPLE_FAMILY.replace("name: iMX7ULP", &format!("name: {chip_name}"));
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn no_descriptions_means_no_opinion() {
        let none = TargetDescriptions::default();
        assert!(none.resolve("STM32U575ZITx").unwrap().is_none());
    }

    #[test]
    fn a_loaded_family_resolves_a_chip_probe_rs_does_not_ship() {
        let file = write_family("FLASHGUI-TEST-1");
        let descriptions = TargetDescriptions::new([file.path().to_path_buf()]);

        let resolved = descriptions.resolve("FLASHGUI-TEST-1").unwrap();
        assert!(
            resolved.is_some(),
            "a chip from a loaded description must be attachable"
        );
    }

    #[test]
    fn a_name_the_descriptions_do_not_carry_falls_through() {
        let file = write_family("FLASHGUI-TEST-1");
        let descriptions = TargetDescriptions::new([file.path().to_path_buf()]);
        // Built-in chips must keep going through the ordinary path, which has
        // the better error messages.
        assert!(descriptions.resolve("NOT-A-REAL-CHIP-AT-ALL").unwrap().is_none());
    }

    #[test]
    fn an_unreadable_file_is_an_error_not_a_silent_fallback() {
        let descriptions = TargetDescriptions::new([PathBuf::from("no/such/file.yaml")]);
        let err = descriptions.resolve("anything").unwrap_err();
        assert!(err.to_string().contains("could not read"));
    }

    #[test]
    fn a_malformed_description_says_which_file() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"this: is: not: a: chip family").unwrap();
        file.flush().unwrap();

        let descriptions = TargetDescriptions::new([file.path().to_path_buf()]);
        let err = descriptions.resolve("anything").unwrap_err();
        assert!(err.to_string().contains("not a valid probe-rs target description"));
    }
}
