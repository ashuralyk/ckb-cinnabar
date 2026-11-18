use std::path::{Path, PathBuf};

const GIT_URL: &str = "https://github.com/ashuralyk/ckb-cinnabar";

fn template_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/contract")
}

fn read_template(name: &str) -> String {
    std::fs::read_to_string(template_root().join(name))
        .unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// Template manifests are `Cargo.toml.liquid` so git checkouts are not scanned
/// as invalid packages. A hand copy renames them and replaces `{{crate_name}}`.
#[test]
fn contract_template_manifest_is_git_only_liquid() {
    let root = template_root();
    assert!(!root.join("Cargo.local.toml").exists());
    assert!(!root.join("hooks").exists());
    assert!(
        !root.join("Cargo.toml").exists(),
        "plain Cargo.toml would be scanned by Cargo git dependency checkouts"
    );
    let config = read_template("cargo-generate.toml");
    assert!(!config.contains("use_local_cinnabar"));
    assert!(!config.contains("cinnabar_path"));

    for rel in [
        "Cargo.toml.liquid",
        "calculator/Cargo.toml.liquid",
        "tests/Cargo.toml.liquid",
        "contracts/{{crate_name}}/Cargo.toml.liquid",
    ] {
        assert!(root.join(rel).is_file(), "missing liquid manifest {rel}");
    }

    let plain: Vec<_> = walkdir_cargo_toml(&root);
    assert!(
        plain.is_empty(),
        "templates/contract must not ship plain Cargo.toml: {plain:?}"
    );

    let manifest = read_template("Cargo.toml.liquid");
    assert!(
        !manifest.contains("{%"),
        "Cargo.toml.liquid must not contain Liquid control tags"
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

fn walkdir_cargo_toml(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
                found.push(path);
            }
        }
    }
    found
}
