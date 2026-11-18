.PHONY: llms-full check-template

# Rebuild llms-full.txt from README.md, AGENTS.md, docs/error-codes.md,
# and CONTRIBUTING.md. The short Core API header lives in the script.
llms-full:
	python3 scripts/gen-llms-full.py

# Render the contract template with git and path dependencies, and simulate
# a hand copy. `--build` also runs make build && make test on the path project.
check-template:
	python3 scripts/check-contract-template.py
