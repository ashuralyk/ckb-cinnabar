use std::path::{Path, PathBuf};

const GIT_URL: &str = "https://github.com/ashuralyk/ckb-cinnabar";

fn template_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/contract")
}

fn read_template(name: &str) -> String {
    std::fs::read_to_string(template_root().join(name))
        .unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// A hand copy replaces `{{crate_name}}` and does not evaluate Liquid tags.
/// `Cargo.toml` must already be the git-dependency manifest.
#[test]
fn contract_template_manifest_has_no_liquid_tags() {
    let manifest = read_template("Cargo.toml");
    assert!(
        !manifest.contains("{%"),
        "Cargo.toml must not contain Liquid control tags"
    );
    assert!(
        !manifest.contains("{{cinnabar_path}}"),
        "path selection lives in Cargo.local.toml, not the copied manifest"
    );
    let rendered = manifest.replace("{{crate_name}}", "common_intention_dispatcher");
    assert!(
        !rendered.contains("{{"),
        "unreplaced placeholder: {rendered}"
    );
    assert!(!rendered.contains("{%"));
    assert_eq!(rendered.matches(GIT_URL).count(), 3, "{rendered}");
    assert!(rendered.contains("name = \"common_intention_dispatcher-runner\""));
    assert!(rendered.contains("\"contracts/common_intention_dispatcher\""));
}

#[test]
fn local_manifest_is_path_dependencies_only() {
    let manifest = read_template("Cargo.local.toml");
    assert!(!manifest.contains("{%"));
    assert!(!manifest.contains("git ="));
    let rendered = manifest
        .replace("{{crate_name}}", "common_intention_dispatcher")
        .replace("{{cinnabar_path}}", "/tmp/cinnabar-checkout");
    assert!(!rendered.contains("{{"), "{rendered}");
    assert!(rendered.contains("ckb-cinnabar = { path = \"/tmp/cinnabar-checkout\" }"));
    assert!(rendered
        .contains("ckb-cinnabar-calculator = { path = \"/tmp/cinnabar-checkout/calculate\" }"));
    assert!(
        rendered.contains("ckb-cinnabar-verifier = { path = \"/tmp/cinnabar-checkout/verify\" }")
    );
}
