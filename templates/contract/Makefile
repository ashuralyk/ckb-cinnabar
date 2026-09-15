# We cannot use $(shell pwd), which will return unix path format on Windows,
# making it hard to use.
cur_dir = $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

TOP := $(cur_dir)
CUSTOM_RUSTFLAGS := -C debug-assertions=yes
CARGO_ARGS := $(CARGO_ARGS)
MODE := release
CLANG := $(shell $(TOP)/scripts/find_clang)
CONTRACT :=
CLEAN_BUILD_DIR_FIRST := true
BUILD_DIR := build/$(MODE)

export CUSTOM_RUSTFLAGS
export TOP
export CARGO_ARGS
export MODE
export CLANG
export BUILD_DIR

default: build test

build:
	@if [ "x$(CLEAN_BUILD_DIR_FIRST)" = "xtrue" ]; then \
		echo "Cleaning $(BUILD_DIR) directory..."; \
		rm -rf $(BUILD_DIR); \
	fi
	mkdir -p $(BUILD_DIR)
	@set -eu; \
	if [ "x$(CONTRACT)" = "x" ]; then \
		for contract in $(wildcard contracts/*); do \
			$(MAKE) -e -C $$contract build; \
		done; \
	else \
		$(MAKE) -e -C contracts/$(CONTRACT) build; \
	fi

test:
	cd tests && cargo test $(CARGO_ARGS) -- --nocapture

prepare:
	rustup target add riscv64imac-unknown-none-elf

clean:
	rm -rf build
	cargo clean

.PHONY: build test prepare clean
