#!/usr/bin/env python3
"""Check that generated and hand-copied contract projects use git dependencies.

- templates/contract ships Cargo.toml.liquid (no plain Cargo.toml) so a git
  checkout of this repository does not emit Cargo package diagnostics
- those liquid manifests list git dependencies and have no Liquid {% %} tags
- cargo generate with no defines (the default) renders .liquid → Cargo.toml
- a hand copy renames .liquid → Cargo.toml, replaces {{crate_name}}, and drops
  cargo-generate.toml
- neither result contains {{ or {%
- a fresh file:// git dependency on this working tree produces no
  templates/contract Cargo error diagnostics

Pass --build to run make build && make test on the generated project.
Those three git dependencies have no rev, branch, or tag. Cargo resolves
them to the default branch of https://github.com/ashuralyk/ckb-cinnabar
at fetch time. The build links that published checkout. It does not compile
this working tree's calculate/ or verify/ crates.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "templates" / "contract"
GIT_URL = "https://github.com/ashuralyk/ckb-cinnabar"
CRATE = "common_intention_dispatcher"
PROJECT = "common-intention-dispatcher"
DEP_NAMES = (
    "ckb-cinnabar",
    "ckb-cinnabar-calculator",
    "ckb-cinnabar-verifier",
)
LIQUID_MANIFESTS = (
    Path("Cargo.toml.liquid"),
    Path("calculator") / "Cargo.toml.liquid",
    Path("tests") / "Cargo.toml.liquid",
    Path("contracts") / "{{crate_name}}" / "Cargo.toml.liquid",
)


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    quiet_stdout: bool = False,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    result = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, env=env)
    if result.stdout and not quiet_stdout:
        print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
    if result.stderr:
        print(result.stderr, end="" if result.stderr.endswith("\n") else "\n", file=sys.stderr)
    if check and result.returncode != 0:
        if quiet_stdout and result.stdout:
            print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
        die(f"command failed ({result.returncode}): {' '.join(cmd)}")
    return result


def cargo_generate() -> list[str]:
    override = os.environ.get("CARGO_GENERATE")
    if override:
        return [override, "generate"]
    return ["cargo", "generate"]


def assert_no_directives(text: str, label: str) -> None:
    if "{%" in text or "{{" in text:
        die(f"{label} still has template directives:\n{text}")


def load_manifest(text: str, label: str) -> dict:
    assert_no_directives(text, label)
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        die(f"{label} is not valid TOML: {exc}\n{text}")


def assert_git(doc: dict, label: str) -> None:
    deps = doc["workspace"]["dependencies"]
    for name in DEP_NAMES:
        spec = deps.get(name)
        if not isinstance(spec, dict) or set(spec) != {"git"} or spec.get("git") != GIT_URL:
            die(f"{label}: expected an unpinned git dependency for {name}, got {spec!r}")


def scan_tree(project: Path) -> None:
    for path in project.rglob("*"):
        if not path.is_file() or ".git" in path.parts:
            continue
        text = path.read_text(errors="replace")
        if "{%" in text or "{{" in text:
            die(f"unresolved template directive in {path}:\n{text}")
        if path.name.endswith(".liquid"):
            die(f"{path} still has a .liquid suffix after generation")


def generate(destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    cmd = cargo_generate() + [
        "--path",
        str(TEMPLATE),
        "--name",
        PROJECT,
        "--destination",
        str(destination),
        "--silent",
    ]
    run(cmd)
    project = destination / PROJECT
    if not project.is_dir():
        die(f"generate did not create {project}")
    return project


def check_project(project: Path, label: str) -> None:
    scan_tree(project)
    for leftover in ("Cargo.local.toml", "cargo-generate.toml", "hooks"):
        if (project / leftover).exists():
            die(f"{label} project still contains generator file {leftover}")
    for liquid in project.rglob("*.liquid"):
        die(f"{label} project still contains {liquid}")
    manifest_path = project / "Cargo.toml"
    if not manifest_path.is_file():
        die(f"{label} project missing Cargo.toml (cargo-generate should strip .liquid)")
    doc = load_manifest(manifest_path.read_text(), f"{label} Cargo.toml")
    if doc["package"]["name"] != f"{CRATE}-runner":
        die(f"{label} package name is {doc['package']['name']!r}")
    members = doc["workspace"]["members"]
    if f"contracts/{CRATE}" not in members:
        die(f"{label} workspace members missing contracts/{CRATE}: {members}")
    assert_git(doc, label)
    run(
        [
            "cargo",
            "metadata",
            "--manifest-path",
            str(manifest_path),
            "--format-version",
            "1",
            "--no-deps",
        ],
        quiet_stdout=True,
    )


def static_template() -> None:
    if (TEMPLATE / "Cargo.local.toml").exists() or (TEMPLATE / "hooks").exists():
        die("path-dependency template files must not exist")
    config = (TEMPLATE / "cargo-generate.toml").read_text()
    if "use_local_cinnabar" in config or "cinnabar_path" in config or "{%" in config:
        die("cargo-generate.toml still configures a local path dependency")

    plain = sorted(TEMPLATE.rglob("Cargo.toml"))
    if plain:
        die(
            "templates/contract must not ship plain Cargo.toml "
            f"(Cargo scans git checkouts); found: {plain}"
        )

    for rel in LIQUID_MANIFESTS:
        path = TEMPLATE / rel
        if not path.is_file():
            die(f"missing template manifest {path}")

    manifest = (TEMPLATE / "Cargo.toml.liquid").read_text()
    if "{%" in manifest or "cinnabar_path" in manifest or "use_local_cinnabar" in manifest:
        die("templates/contract/Cargo.toml.liquid contains Liquid control tags or a path option")
    if "path =" in manifest:
        die("templates/contract/Cargo.toml.liquid must not use path dependencies")
    rendered = manifest.replace("{{crate_name}}", CRATE)
    assert_git(load_manifest(rendered, "template Cargo.toml.liquid"), "template")


def materialize_hand_copy_tree(dest: Path) -> None:
    """Copy the template and apply the documented hand-copy steps."""
    shutil.copytree(TEMPLATE, dest)
    for path in sorted(dest.rglob("Cargo.toml.liquid")):
        path.rename(path.with_name("Cargo.toml"))
    for path in dest.rglob("*"):
        if path.is_file() and "{{crate_name}}" in path.read_text(errors="replace"):
            path.write_text(path.read_text().replace("{{crate_name}}", CRATE))
    crate_dir = dest / "contracts" / "{{crate_name}}"
    if not crate_dir.is_dir():
        die("hand copy is missing contracts/{{crate_name}}")
    crate_dir.rename(dest / "contracts" / CRATE)
    (dest / "cargo-generate.toml").unlink()


def simulate_hand_copy(tmp: Path) -> None:
    dest = tmp / "hand-copy"
    materialize_hand_copy_tree(dest)
    check_project(dest, "hand copy")
    print("hand copy: git manifest ok")


def copy_working_tree(dest: Path) -> None:
    """Copy this checkout for a file:// git dependency, excluding build junk."""
    ignore = shutil.ignore_patterns(
        "target",
        "build",
        ".git",
        "*.rs.bk",
        ".cursor",
    )
    shutil.copytree(ROOT, dest, ignore=ignore)
    # Ensure the copied tree still has no plain template Cargo.toml.
    plain = sorted((dest / "templates" / "contract").rglob("Cargo.toml"))
    if plain:
        die(f"copied working tree still has plain template Cargo.toml: {plain}")


def check_git_dependency_scan(tmp: Path) -> None:
    """Fresh git dep on this working tree must not emit template Cargo errors."""
    repo = tmp / "cinnabar-src"
    copy_working_tree(repo)
    run(["git", "init"], cwd=repo, quiet_stdout=True)
    run(["git", "add", "-A"], cwd=repo, quiet_stdout=True)
    run(
        [
            "git",
            "-c",
            "user.name=cinnabar-check",
            "-c",
            "user.email=cinnabar-check@example.com",
            "commit",
            "-m",
            "template scan fixture",
        ],
        cwd=repo,
        quiet_stdout=True,
    )
    rev = run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        quiet_stdout=True,
    ).stdout.strip()
    if not rev:
        die("failed to resolve fixture HEAD")

    consumer = tmp / "git-dep-consumer"
    consumer.mkdir()
    (consumer / "src").mkdir()
    (consumer / "src" / "lib.rs").write_text("#![no_std]\n")
    repo_uri = repo.resolve().as_uri()
    (consumer / "Cargo.toml").write_text(
        "\n".join(
            [
                "[package]",
                'name = "git_dep_consumer"',
                'version = "0.1.0"',
                'edition = "2021"',
                "",
                "[dependencies]",
                f'ckb-cinnabar-verifier = {{ git = "{repo_uri}", rev = "{rev}" }}',
                "",
            ]
        )
    )

    cargo_home = tmp / "cargo-home"
    cargo_home.mkdir()
    env = os.environ.copy()
    env["CARGO_HOME"] = str(cargo_home)
    env["CARGO_TERM_COLOR"] = "never"
    result = run(
        ["cargo", "fetch", "--manifest-path", str(consumer / "Cargo.toml")],
        check=False,
        env=env,
    )
    combined = f"{result.stdout}\n{result.stderr}"
    if result.returncode != 0:
        die(f"cargo fetch against working-tree git dep failed:\n{combined}")

    # Match the downstream symptom: Cargo `error:` diagnostics that cite the
    # template tree or unresolved {{crate_name}} placeholders.
    lines = combined.splitlines()
    for i, line in enumerate(lines):
        if not line.startswith("error:"):
            continue
        window = "\n".join(lines[i : i + 8])
        if "templates/contract" in window or "{{crate_name}}" in window:
            die(f"template-derived Cargo error diagnostic:\n{window}")
    if "{{crate_name}}" in combined:
        die(
            "git dependency output still mentions unresolved {{crate_name}}:\n"
            + "\n".join(line for line in lines if "{{crate_name}}" in line)
        )

    print("git dependency scan: no template Cargo diagnostics")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--build",
        action="store_true",
        help=(
            "run make build && make test on the generated project; "
            "git dependencies resolve to the default branch, not this commit"
        ),
    )
    args = parser.parse_args()

    static_template()
    print("template source: Cargo.toml.liquid git manifests, no plain Cargo.toml")

    with tempfile.TemporaryDirectory(prefix="cinnabar-template-") as raw:
        tmp = Path(raw)
        check_git_dependency_scan(tmp)
        simulate_hand_copy(tmp)

        project = generate(tmp / "default")
        check_project(project, "cargo generate")
        run(["cargo", "generate-lockfile", "--manifest-path", str(project / "Cargo.toml")])
        print("cargo generate: default git dependencies ok")

        if args.build:
            print(
                "note: --build links the default-branch git checkout, "
                "not this working tree",
                flush=True,
            )
            env = os.environ.copy()
            env.setdefault("CARGO_TERM_COLOR", "always")
            env.setdefault("TARGET_CC", "clang")
            print("+ make -C", project, "build test CARGO_ARGS=--locked", flush=True)
            result = subprocess.run(
                ["make", "build", "test", "CARGO_ARGS=--locked"],
                cwd=project,
                env=env,
            )
            if result.returncode != 0:
                die(f"make build test failed ({result.returncode})")
            print("generated project: make build && make test ok")

    print("contract template checks passed")


if __name__ == "__main__":
    main()
