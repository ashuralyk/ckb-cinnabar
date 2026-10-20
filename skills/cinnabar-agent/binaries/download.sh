#!/usr/bin/env bash
# Download the WarSporeSaga FakeRpc binaries and check their SHA-256.
# The blobs used to be committed next to this script. They remain reachable
# from the last commit that stored them, pinned below.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
REV="cacde02d7fffa37ffcd5a853c9fd785679580850"
BASE="https://raw.githubusercontent.com/ashuralyk/ckb-cinnabar/${REV}/skills/cinnabar-agent/binaries"

cd "$ROOT"
for name in cluster spore type_burn xudt; do
  echo "downloading ${name}"
  curl -fsSL "${BASE}/${name}" -o "${name}.partial"
  mv "${name}.partial" "${name}"
done

sha256sum -c checksums.sha256
echo "ok: cluster spore type_burn xudt"
