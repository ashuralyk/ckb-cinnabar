#!/usr/bin/env python3
"""Check that generated and hand-copied contract projects use git dependencies.

- templates/contract/Cargo.toml lists git dependencies and has no Liquid tags
- cargo generate with no defines (the default)
- a hand copy that only replaces {{crate_name}} and drops cargo-generate.toml
- neither result contains {{ or {%

Pass --build to run make build && make test on the generated project.
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


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    quiet_stdout: bool = False,
) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    result = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)
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
        if not isinstance(spec, dict) or spec.get("git") != GIT_URL or "path" in spec:
            die(f"{label}: expected git dependency for {name}, got {spec!r}")


def scan_tree(project: Path) -> None:
    for path in project.rglob("*"):
        if not path.is_file() or ".git" in path.parts:
            continue
        text = path.read_text(errors="replace")
        if "{%" in text or "{{" in text:
            die(f"unresolved template directive in {path}:\n{text}")


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
    manifest_path = project / "Cargo.toml"
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
    manifest = (TEMPLATE / "Cargo.toml").read_text()
    if "{%" in manifest or "cinnabar_path" in manifest or "use_local_cinnabar" in manifest:
        die("templates/contract/Cargo.toml contains Liquid tags or a path option")
    if "path =" in manifest:
        die("templates/contract/Cargo.toml must not use path dependencies")
    rendered = manifest.replace("{{crate_name}}", CRATE)
    assert_git(load_manifest(rendered, "template Cargo.toml"), "template")


def simulate_hand_copy(tmp: Path) -> None:
    dest = tmp / "hand-copy"
    shutil.copytree(TEMPLATE, dest)
    for path in dest.rglob("*"):
        if path.is_file() and "{{crate_name}}" in path.read_text(errors="replace"):
            path.write_text(path.read_text().replace("{{crate_name}}", CRATE))
    crate_dir = dest / "contracts" / "{{crate_name}}"
    if not crate_dir.is_dir():
        die("hand copy is missing contracts/{{crate_name}}")
    crate_dir.rename(dest / "contracts" / CRATE)
    (dest / "cargo-generate.toml").unlink()
    check_project(dest, "hand copy")
    print("hand copy: git manifest ok")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--build",
        action="store_true",
        help="run make build && make test on the generated project",
    )
    args = parser.parse_args()

    static_template()
    print("template source: git manifest has no Liquid tags")

    with tempfile.TemporaryDirectory(prefix="cinnabar-template-") as raw:
        tmp = Path(raw)
        simulate_hand_copy(tmp)

        project = generate(tmp / "default")
        check_project(project, "cargo generate")
        run(["cargo", "generate-lockfile", "--manifest-path", str(project / "Cargo.toml")])
        print("cargo generate: default git dependencies ok")

        if args.build:
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
