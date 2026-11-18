#!/usr/bin/env python3
"""Render the contract template and reject unresolved Liquid directives.

Checks:
- templates/contract/Cargo.toml is the git manifest and has no {% %} tags
- cargo generate with git dependencies, path dependencies, and the default
- a hand copy that only replaces {{crate_name}} and drops generator files
- a relative cinnabar_path is rejected

Pass --build to run make build && make test on the path-dependency project.
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


def assert_paths(doc: dict, label: str, checkout: Path) -> None:
    root = checkout.resolve()
    expected = {
        "ckb-cinnabar": str(root),
        "ckb-cinnabar-calculator": str(root / "calculate"),
        "ckb-cinnabar-verifier": str(root / "verify"),
    }
    deps = doc["workspace"]["dependencies"]
    for name, path in expected.items():
        spec = deps.get(name)
        if not isinstance(spec, dict) or spec.get("path") != path or "git" in spec:
            die(f"{label}: expected path {path} for {name}, got {spec!r}")


def scan_tree(project: Path) -> None:
    for path in project.rglob("*"):
        if not path.is_file() or ".git" in path.parts:
            continue
        text = path.read_text(errors="replace")
        if "{%" in text or "{{" in text:
            die(f"unresolved template directive in {path}:\n{text}")


def generate(destination: Path, defines: list[tuple[str, str]]) -> Path:
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
    for key, value in defines:
        cmd.extend(["--define", f"{key}={value}"])
    run(cmd)
    project = destination / PROJECT
    if not project.is_dir():
        die(f"generate did not create {project}")
    return project


def check_project(project: Path, kind: str, checkout: Path | None) -> None:
    scan_tree(project)
    for leftover in ("Cargo.local.toml", "cargo-generate.toml", "hooks"):
        if (project / leftover).exists():
            die(f"{kind} project still contains generator file {leftover}")
    manifest_path = project / "Cargo.toml"
    doc = load_manifest(manifest_path.read_text(), f"{kind} Cargo.toml")
    if doc["package"]["name"] != f"{CRATE}-runner":
        die(f"{kind} package name is {doc['package']['name']!r}")
    members = doc["workspace"]["members"]
    if f"contracts/{CRATE}" not in members:
        die(f"{kind} workspace members missing contracts/{CRATE}: {members}")
    if kind == "git":
        assert_git(doc, kind)
    else:
        assert checkout is not None
        assert_paths(doc, kind, checkout)
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
    manifest = (TEMPLATE / "Cargo.toml").read_text()
    if "{%" in manifest or "{{cinnabar_path}}" in manifest:
        die("templates/contract/Cargo.toml contains Liquid tags or cinnabar_path")
    rendered = manifest.replace("{{crate_name}}", CRATE)
    assert_git(load_manifest(rendered, "template Cargo.toml"), "template")

    local = (TEMPLATE / "Cargo.local.toml").read_text()
    if "{%" in local or "git =" in local:
        die("Cargo.local.toml must be path dependencies without Liquid tags")
    local_rendered = local.replace("{{crate_name}}", CRATE).replace(
        "{{cinnabar_path}}", "/tmp/cinnabar-checkout"
    )
    doc = load_manifest(local_rendered, "Cargo.local.toml")
    spec = doc["workspace"]["dependencies"]["ckb-cinnabar"]
    if spec.get("path") != "/tmp/cinnabar-checkout":
        die(f"Cargo.local.toml path was {spec!r}")
    hook = (TEMPLATE / "hooks" / "select-deps.rhai").read_text()
    if "Cargo.local.toml" not in hook or "use_local_cinnabar" not in hook:
        die("hooks/select-deps.rhai does not select the local manifest")


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
    for extra in ("cargo-generate.toml", "Cargo.local.toml"):
        (dest / extra).unlink()
    shutil.rmtree(dest / "hooks")
    check_project(dest, "git", None)
    print("hand copy: git manifest ok")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--build",
        action="store_true",
        help="run make build && make test on the path-dependency project",
    )
    args = parser.parse_args()

    static_template()
    print("template source: git manifest has no Liquid tags")

    with tempfile.TemporaryDirectory(prefix="cinnabar-template-") as raw:
        tmp = Path(raw)
        simulate_hand_copy(tmp)

        git_project = generate(tmp / "git", [("use_local_cinnabar", "false")])
        check_project(git_project, "git", None)
        run(["cargo", "generate-lockfile", "--manifest-path", str(git_project / "Cargo.toml")])
        print("cargo generate: git dependencies ok")

        default_project = generate(tmp / "default", [])
        check_project(default_project, "git", None)
        print("cargo generate: default is git dependencies")

        local_project = generate(
            tmp / "local",
            [("use_local_cinnabar", "true"), ("cinnabar_path", str(ROOT.resolve()))],
        )
        check_project(local_project, "path", ROOT)
        print("cargo generate: path dependencies ok")

        relative = generate_expect_fail(
            tmp / "relative",
            [("use_local_cinnabar", "true"), ("cinnabar_path", "../ckb-cinnabar")],
        )
        if "absolute" not in relative.lower():
            die(f"relative cinnabar_path failed without an absolute-path error:\n{relative}")
        print("cargo generate: relative cinnabar_path rejected")

        if args.build:
            run(["cargo", "generate-lockfile", "--manifest-path", str(local_project / "Cargo.toml")])
            env = os.environ.copy()
            env.setdefault("CARGO_TERM_COLOR", "always")
            env.setdefault("TARGET_CC", "clang")
            print("+ make -C", local_project, "build test CARGO_ARGS=--locked", flush=True)
            result = subprocess.run(
                ["make", "build", "test", "CARGO_ARGS=--locked"],
                cwd=local_project,
                env=env,
            )
            if result.returncode != 0:
                die(f"make build test failed ({result.returncode})")
            print("path project: make build && make test ok")

    print("contract template checks passed")


def generate_expect_fail(destination: Path, defines: list[tuple[str, str]]) -> str:
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
    for key, value in defines:
        cmd.extend(["--define", f"{key}={value}"])
    result = run(cmd, check=False)
    if result.returncode == 0:
        die("relative cinnabar_path was accepted")
    return (result.stderr or "") + (result.stdout or "")


if __name__ == "__main__":
    main()
