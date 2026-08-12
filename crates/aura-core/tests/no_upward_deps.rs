#[test]
fn core_has_no_workspace_dependencies() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("manifest");
    for forbidden in [
        "aura-catalog",
        "aura-ingest",
        "aura-jobs",
        "aura-app",
        "aura-perf",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "aura-core must not depend on {forbidden}"
        );
    }
}
