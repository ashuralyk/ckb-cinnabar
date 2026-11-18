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
/// Workspace dependencies are the three git lines.
#[test]
fn contract_template_manifest_is_git_only() {
    let root = template_root();
    assert!(!root.join("Cargo.local.toml").exists());
    assert!(!root.join("hooks").exists());
    let config = read_template("cargo-generate.toml");
    assert!(!config.contains("use_local_cinnabar"));
    assert!(!config.contains("cinnabar_path"));

    let manifest = read_template("Cargo.toml");
    assert!(
        !manifest.contains("{%"),
        "Cargo.toml must not contain Liquid control tags"
    );
    assert!(!manifest.contains("path ="));
    assert!(!manifest.contains("cinnabar_path"));
    assert!(!manifest.contains("use_local_cinnabar"));
    let rendered = manifest.replace("{{crate_name}}", "common_intention_dispatcher");
    assert!(
        !rendered.contains("{{"),
        "unreplaced placeholder: {rendered}"
    );
    assert!(!rendered.contains("{%"));
    assert_eq!(rendered.matches(GIT_URL).count(), 3, "{rendered}");
    for name in [
        "ckb-cinnabar",
        "ckb-cinnabar-calculator",
        "ckb-cinnabar-verifier",
    ] {
        let line = format!("{name} = {{ git = \"{GIT_URL}\" }}");
        assert!(
            rendered.contains(&line),
            "missing unpinned git dependency `{line}` in {rendered}"
        );
    }
    assert!(rendered.contains("name = \"common_intention_dispatcher-runner\""));
    assert!(rendered.contains("\"contracts/common_intention_dispatcher\""));
}
