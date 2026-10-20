.PHONY: llms-full

# Rebuild llms-full.txt from README.md, AGENTS.md, docs/error-codes.md,
# and CONTRIBUTING.md. The short Core API header lives in the script.
llms-full:
	python3 scripts/gen-llms-full.py
