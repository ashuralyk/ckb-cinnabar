.PHONY: llms-full check-template

# Rebuild llms-full.txt from README.md, AGENTS.md, docs/error-codes.md,
# and CONTRIBUTING.md. The short Core API header lives in the script.
llms-full:
	python3 scripts/gen-llms-full.py

# Render the contract template with its default git dependencies, simulate a
# hand copy (including Cargo.toml.liquid → Cargo.toml), and assert a fresh
# git-dependency scan emits no template Cargo diagnostics. `--build` also
# runs make build && make test. Those dependencies have no rev, so that build
# uses the default branch at fetch time.
check-template:
	python3 scripts/check-contract-template.py
