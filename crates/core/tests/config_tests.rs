use std::path::PathBuf;
use real_dlio_core::Config;

fn fixture_path(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR for this crate → real_dlio/crates/core
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn parse_simple_workload() {
    let path = fixture_path("test_workload1.yaml");
    let cfg = Config::from_yaml_file(&path).expect("should load test_workload1.yaml");
    let raw = &cfg.raw;
    // basic smoke‐check: those keys exist
    assert!(raw.get("dataset").is_some(), "dataset key missing");
    assert!(raw.get("reader").is_some(), "reader key missing");
    assert_eq!(
        raw["dataset"]["name"].as_str().unwrap(),
        "test",
        "dataset.name"
    );
    assert_eq!(raw["dataset"]["size"].as_i64().unwrap(), 1024, "dataset.size");
}

#[test]
fn parse_unet3d_sample() {
    let path = fixture_path("unet3d_h100.yaml");
    let cfg = Config::from_yaml_file(&path).expect("should load unet3d_h100.yaml");
    let raw = &cfg.raw;
    // e.g. check that the top‐level workflow key exists
    assert!(raw.get("workflow").is_some(), "workflow missing in unet3d");
    // and maybe dataset.name is "unet3d"
    if let Some(name) = raw["dataset"]["name"].as_str() {
        assert_eq!(name, "unet3d");
    }
}

