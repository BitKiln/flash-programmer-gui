use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::exit_codes::CliError;

fn default_schema_version() -> u32 {
    1
}

fn default_target() -> String {
    "STM32F401RE".to_string()
}

fn default_interface() -> String {
    "SWD".to_string()
}

fn default_speed_khz() -> u32 {
    2000
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

/// Metadata section in TOML profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileMetadata {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default = "default_interface")]
    pub interface: String,
    #[serde(default = "default_speed_khz")]
    pub speed_khz: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_id: Option<String>,
}

/// Firmware section in TOML profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirmwareConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_address: Option<String>,
}

/// Options section in TOML profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileOptions {
    #[serde(default = "default_true")]
    pub verify_after: bool,
    #[serde(default = "default_true")]
    pub reset_after: bool,
    #[serde(default = "default_false")]
    pub full_chip_erase: bool,
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self {
            verify_after: true,
            reset_after: true,
            full_chip_erase: false,
        }
    }
}

/// Reusable target connection and flashing profile (TOML format).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashProfile {
    pub profile: ProfileMetadata,
    #[serde(default)]
    pub firmware: FirmwareConfig,
    #[serde(default)]
    pub options: ProfileOptions,
}

impl FlashProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        description: Option<String>,
        target: impl Into<String>,
        interface: impl Into<String>,
        speed_khz: u32,
        probe_id: Option<String>,
        default_path: Option<String>,
        base_address: Option<String>,
        verify_after: bool,
        reset_after: bool,
        full_chip_erase: bool,
    ) -> Self {
        Self {
            profile: ProfileMetadata {
                schema_version: 1,
                name: name.into(),
                description,
                target: target.into(),
                interface: interface.into(),
                speed_khz,
                probe_id,
            },
            firmware: FirmwareConfig {
                default_path,
                base_address,
            },
            options: ProfileOptions {
                verify_after,
                reset_after,
                full_chip_erase,
            },
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.profile.name
    }

    #[inline]
    pub fn description(&self) -> Option<&str> {
        self.profile.description.as_deref()
    }

    #[inline]
    pub fn target(&self) -> &str {
        &self.profile.target
    }

    #[inline]
    pub fn interface(&self) -> &str {
        &self.profile.interface
    }

    #[inline]
    pub fn speed_khz(&self) -> u32 {
        self.profile.speed_khz
    }

    #[inline]
    pub fn probe_id(&self) -> Option<&str> {
        self.profile.probe_id.as_deref()
    }

    #[inline]
    pub fn default_path(&self) -> Option<&str> {
        self.firmware.default_path.as_deref()
    }

    #[inline]
    pub fn base_address(&self) -> Option<&str> {
        self.firmware.base_address.as_deref()
    }

    #[inline]
    pub fn verify_after(&self) -> bool {
        self.options.verify_after
    }

    #[inline]
    pub fn reset_after(&self) -> bool {
        self.options.reset_after
    }

    #[inline]
    pub fn full_chip_erase(&self) -> bool {
        self.options.full_chip_erase
    }
}

/// Summary item when listing available profiles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub name: String,
    pub description: Option<String>,
    pub target: String,
    pub file_path: String,
}

/// Returns the local project profile directory: `./.flashgui/profiles`
pub fn local_profiles_dir() -> PathBuf {
    PathBuf::from(".flashgui").join("profiles")
}

/// Returns the user global configuration profile directory: `<user_config>/flashgui/profiles`
pub fn user_profiles_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().join("flashgui").join("profiles"))
}

/// Resolves the filesystem path for a profile by name or explicit file path.
pub fn resolve_profile_path(
    name: &str,
    custom_profile_file: Option<&Path>,
) -> Result<PathBuf, CliError> {
    // 1. If explicit custom profile file was provided, use it
    if let Some(custom) = custom_profile_file {
        if custom.exists() {
            return Ok(custom.to_path_buf());
        }
        return Err(CliError::InvalidArgsOrProfile(format!(
            "Specified profile file '{}' does not exist",
            custom.display()
        )));
    }

    let file_name = if name.ends_with(".toml") {
        name.to_string()
    } else {
        format!("{}.toml", name)
    };

    // 2. Check local project directory (./.flashgui/profiles/<name>.toml)
    let local_path = local_profiles_dir().join(&file_name);
    if local_path.is_file() {
        return Ok(local_path);
    }

    // 3. Check direct relative or absolute path if name looks like a path
    let direct_path = PathBuf::from(name);
    if direct_path.is_file() {
        return Ok(direct_path);
    }

    // 4. Check user configuration directory (~/.config/flashgui/profiles/<name>.toml)
    if let Some(user_dir) = user_profiles_dir() {
        let user_path = user_dir.join(&file_name);
        if user_path.is_file() {
            return Ok(user_path);
        }
    }

    Err(CliError::InvalidArgsOrProfile(format!(
        "Profile '{}' not found in local (.flashgui/profiles) or user configuration directory",
        name
    )))
}

/// Loads a profile from disk by name or custom file path.
pub fn load_profile(
    name: &str,
    custom_profile_file: Option<&Path>,
) -> Result<FlashProfile, CliError> {
    let path = resolve_profile_path(name, custom_profile_file)?;
    let content = fs::read_to_string(&path).map_err(|e| {
        CliError::InvalidArgsOrProfile(format!(
            "Failed to read profile file '{}': {}",
            path.display(),
            e
        ))
    })?;

    toml::from_str::<FlashProfile>(&content).map_err(|e| {
        CliError::InvalidArgsOrProfile(format!(
            "Failed to parse TOML profile '{}': {}",
            path.display(),
            e
        ))
    })
}

/// Saves a profile to disk. If `custom_profile_file` is specified, writes to it.
/// Otherwise writes to `./.flashgui/profiles` if `.flashgui` exists, or user config directory.
pub fn save_profile(
    profile: &FlashProfile,
    custom_profile_file: Option<&Path>,
) -> Result<PathBuf, CliError> {
    let target_path = if let Some(custom) = custom_profile_file {
        custom.to_path_buf()
    } else {
        let file_name = format!("{}.toml", profile.name());
        let local_flashgui = PathBuf::from(".flashgui");
        if local_flashgui.exists() || !user_profiles_dir().is_some() {
            local_profiles_dir().join(file_name)
        } else {
            user_profiles_dir()
                .unwrap_or_else(local_profiles_dir)
                .join(file_name)
        }
    };

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CliError::InvalidArgsOrProfile(format!(
                "Failed to create profile directory '{}': {}",
                parent.display(),
                e
            ))
        })?;
    }

    let toml_str = toml::to_string_pretty(profile).map_err(|e| {
        CliError::InvalidArgsOrProfile(format!("Failed to serialize profile to TOML: {}", e))
    })?;

    fs::write(&target_path, toml_str).map_err(|e| {
        CliError::InvalidArgsOrProfile(format!(
            "Failed to write profile file '{}': {}",
            target_path.display(),
            e
        ))
    })?;

    Ok(target_path)
}

/// Lists all available profiles from custom file, local project, and user configuration directories.
pub fn list_profiles(custom_profile_file: Option<&Path>) -> Result<Vec<ProfileSummary>, CliError> {
    let mut summaries = BTreeMap::new();

    // 1. Check custom profile file if provided
    if let Some(custom) = custom_profile_file {
        if custom.is_file() {
            if let Ok(content) = fs::read_to_string(custom) {
                if let Ok(prof) = toml::from_str::<FlashProfile>(&content) {
                    summaries.insert(
                        prof.name().to_string(),
                        ProfileSummary {
                            name: prof.name().to_string(),
                            description: prof.description().map(ToString::to_string),
                            target: prof.target().to_string(),
                            file_path: custom.to_string_lossy().to_string(),
                        },
                    );
                }
            }
        }
    }

    // Helper to scan a directory for *.toml profiles
    let mut scan_dir = |dir: &Path| {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(prof) = toml::from_str::<FlashProfile>(&content) {
                            summaries.entry(prof.name().to_string()).or_insert_with(|| {
                                ProfileSummary {
                                    name: prof.name().to_string(),
                                    description: prof.description().map(ToString::to_string),
                                    target: prof.target().to_string(),
                                    file_path: path.to_string_lossy().to_string(),
                                }
                            });
                        }
                    }
                }
            }
        }
    };

    // 2. Scan local profiles directory
    scan_dir(&local_profiles_dir());

    // 3. Scan user profiles directory
    if let Some(user_dir) = user_profiles_dir() {
        scan_dir(&user_dir);
    }

    Ok(summaries.into_values().collect())
}

/// Deletes a profile by name or from custom file path.
pub fn delete_profile(
    name: &str,
    custom_profile_file: Option<&Path>,
) -> Result<PathBuf, CliError> {
    let path = resolve_profile_path(name, custom_profile_file)?;
    fs::remove_file(&path).map_err(|e| {
        CliError::InvalidArgsOrProfile(format!(
            "Failed to delete profile file '{}': {}",
            path.display(),
            e
        ))
    })?;
    Ok(path)
}
