#[test]
fn core_has_no_workspace_dependencies() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("manifest");
    // Every workspace crate that exists, so the rule keeps its teeth as the
    // workspace grows. A phase that adds a crate adds it here; phase 05 added the
    // last two.
    for forbidden in [
        "aura-catalog",
        "aura-ingest",
        "aura-jobs",
        "aura-app",
        "aura-perf",
        "aura-cache",
        "aura-cli",
        "aura-cloud",
        "aura-infer",
        "aura-models",
        "aura-preview",
        "aura-raw",
        "aura-index",
        "aura-vision",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "aura-core must not depend on {forbidden}"
        );
    }
}
