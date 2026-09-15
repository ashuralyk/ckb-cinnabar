cur_dir = $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

TOP := $(cur_dir)
CUSTOM_RUSTFLAGS := -C debug-assertions=yes
FULL_RUSTFLAGS := -C target-feature=+zba,+zbb,+zbc,+zbs -C passes=lower-atomic $(CUSTOM_RUSTFLAGS)
CARGO_ARGS :=
MODE := release
CLANG := $(shell $(TOP)/scripts/find_clang)
BUILD_DIR :=
BINARIES := $(notdir $(shell pwd))
CARGO_TARGET_DIR ?= $(TOP)/target

export CARGO_TARGET_DIR

ifeq (release,$(MODE))
	MODE_ARGS := --release
endif

default: build

build:
	RUSTFLAGS="$(FULL_RUSTFLAGS)" TARGET_CC="$(CLANG)" \
		cargo build --target=riscv64imac-unknown-none-elf $(MODE_ARGS) $(CARGO_ARGS)
	@set -eu; \
	if [ "x$(BUILD_DIR)" != "x" ]; then \
		for binary in $(BINARIES); do \
			echo "Copying binary $$binary to build directory"; \
			cp "$(CARGO_TARGET_DIR)/riscv64imac-unknown-none-elf/$(MODE)/$$binary" "$(TOP)/$(BUILD_DIR)"; \
		done \
	fi

prepare:
	rustup target add riscv64imac-unknown-none-elf

.PHONY: build prepare
