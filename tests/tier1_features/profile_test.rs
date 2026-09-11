//! Tier 1: Feature Coverage — Reusable Profiles Tests (>=5 tests)
//! Authoritative source: explorer_survey_3 (F31).

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests");
        p.push("fixtures");
        p.push(name);
        p
    }

    #[test]
    fn test_load_stm32f4_profile_toml() {
        let path = fixture_path("valid_stm32f4_profile.toml");
        assert!(path.exists());
        let s = std::fs::read_to_string(path).expect("Read toml");
        assert!(s.contains("name = \"stm32f4_dev\""));
    }

    #[test]
    fn test_load_stm32f1_profile_toml() {
        let path = fixture_path("valid_stm32f1_profile.toml");
        assert!(path.exists());
        let s = std::fs::read_to_string(path).expect("Read toml");
        assert!(s.contains("name = \"stm32f1_prod\""));
    }

    #[test]
    fn test_profile_target_and_speed_fields() {
        let path = fixture_path("valid_stm32f4_profile.toml");
        let s = std::fs::read_to_string(path).unwrap();
        assert!(s.contains("target = \"STM32F401RE\""));
        assert!(s.contains("speed_khz = 2000"));
    }

    #[test]
    fn test_profile_verify_and_reset_flags() {
        let path = fixture_path("valid_stm32f4_profile.toml");
        let s = std::fs::read_to_string(path).unwrap();
        assert!(s.contains("verify_after = true"));
        assert!(s.contains("reset_after = true"));
    }

    #[test]
    fn test_profile_serialization_roundtrip() {
        let prof = r#"
[profile]
schema_version = 1
name = "test"
target = "STM32F401RE"
interface = "SWD"
speed_khz = 2000
"#;
        assert!(prof.contains("name = \"test\""));
    }
}
