//! Opaque-box E2E Test Suite Runner
//! Consolidates Tiers 1-4 for seamless cargo test execution.

#[path = "tier1_features/mod.rs"]
pub mod tier1_features;

#[path = "tier2_boundaries/mod.rs"]
pub mod tier2_boundaries;

#[path = "tier3_combinations/mod.rs"]
pub mod tier3_combinations;

#[path = "tier4_workloads/mod.rs"]
pub mod tier4_workloads;

#[test]
fn test_e2e_infrastructure_ready() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = std::path::Path::new(manifest_dir).join("tests").join("fixtures");
    assert!(fixture_dir.exists(), "Fixtures directory must exist at {:?}", fixture_dir);
}
