# Toolchain image for Cinnabar contract projects.
# On-chain target: riscv64imac-unknown-none-elf. Contract Makefiles call
# `scripts/find_clang`, which accepts only clang 16+ (rustc
# `-C passes=lower-atomic`). Debian bookworm's `clang` package is 14, so
# this image installs clang-18 from apt.llvm.org and exposes it as `clang`.
FROM rust:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        make \
        pkg-config \
        libssl-dev \
    && curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key \
        | gpg --dearmor -o /usr/share/keyrings/llvm-archive-keyring.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/llvm-archive-keyring.gpg] http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-18 main" \
        > /etc/apt/sources.list.d/llvm-18.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends clang-18 \
    && ln -sf /usr/bin/clang-18 /usr/local/bin/clang \
    && ln -sf /usr/bin/clang++-18 /usr/local/bin/clang++ \
    && clang --version \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add riscv64imac-unknown-none-elf \
    && cargo install cargo-generate --version 0.20.0

# Pinned ckb-cli release (interactive signing). Headless deploy can skip it
# with --dry-run or --privkey-env.
ARG CKB_CLI_VERSION=1.15.0
RUN curl -fsSL -o /tmp/ckb-cli.tar.gz \
        "https://github.com/nervosnetwork/ckb-cli/releases/download/v${CKB_CLI_VERSION}/ckb-cli_v${CKB_CLI_VERSION}_x86_64-unknown-linux-gnu.tar.gz" \
    && tar -xzf /tmp/ckb-cli.tar.gz -C /tmp \
    && mv /tmp/ckb-cli_v${CKB_CLI_VERSION}_x86_64-unknown-linux-gnu/ckb-cli /usr/local/bin/ckb-cli \
    && rm -rf /tmp/ckb-cli.tar.gz /tmp/ckb-cli_v${CKB_CLI_VERSION}_x86_64-unknown-linux-gnu \
    && ckb-cli --version

WORKDIR /workspace
