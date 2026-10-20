# Toolchain image for Cinnabar contract projects.
# On-chain target: riscv64imac-unknown-none-elf. Contract Makefiles also need
# clang (ckb-std) and pass -C passes=lower-atomic.
FROM rust:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        clang \
        curl \
        make \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add riscv64imac-unknown-none-elf \
    && cargo install cargo-generate --version 0.20.0 --locked

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
