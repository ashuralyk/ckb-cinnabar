// Choose git dependencies (Cargo.toml) or path dependencies (Cargo.local.toml).
// Cargo.toml stays valid TOML with git dependencies so a hand copy never
// has to evaluate Liquid control tags. This script runs after substitution.
let use_local = variable::get("use_local_cinnabar");
if use_local == true {
    let path = variable::get("cinnabar_path");
    if path == "" {
        throw "use_local_cinnabar requires cinnabar_path, an absolute path to the cinnabar checkout";
    }
    if !path.starts_with("/") {
        throw "cinnabar_path must be absolute (Cargo resolves it from the generated project): " + path;
    }
    if path.contains("\"") || path.contains("\n") || path.contains("\r") {
        throw "cinnabar_path cannot contain quotes or newlines";
    }
    file::delete("Cargo.toml");
    file::rename("Cargo.local.toml", "Cargo.toml");
} else {
    file::delete("Cargo.local.toml");
}

// Drop the generator script. cargo-generate also removes hook files afterwards.
file::delete("hooks/select-deps.rhai");
file::delete("hooks");
