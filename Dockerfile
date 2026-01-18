# MielinOS Development Container
# Multi-stage build for efficient development and testing

FROM rust:1.83 as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust tools
RUN cargo install cargo-nextest --locked
RUN rustup target add wasm32-unknown-unknown

WORKDIR /mielin

# Copy workspace files
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .rustfmt.toml ./

# Copy all crates
COPY mielin-kernel ./mielin-kernel
COPY mielin-hal ./mielin-hal
COPY mielin-rt ./mielin-rt
COPY mielin-mesh ./mielin-mesh
COPY mielin-cells ./mielin-cells
COPY mielin-wasm ./mielin-wasm
COPY mielin-tensor ./mielin-tensor
COPY mielin-cli ./mielin-cli
COPY benches ./benches
COPY examples ./examples

# Build all crates
RUN cargo build --release --workspace

# Run tests
RUN cargo nextest run --workspace

# Runtime stage for mesh-cluster
FROM debian:bookworm-slim as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy mesh-cluster binary from builder
COPY --from=builder /mielin/target/release/node /app/node

# Set up non-root user
RUN useradd -m -u 1000 mielin && \
    chown -R mielin:mielin /app

USER mielin

# Expose default QUIC port
EXPOSE 8080/udp

# Default command
CMD ["/app/node", "--help"]

# Development stage
FROM rust:1.83-slim as development

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Install development tools
RUN cargo install cargo-nextest --locked \
    && cargo install cargo-watch \
    && rustup target add wasm32-unknown-unknown

WORKDIR /mielin

# Copy built artifacts from builder
COPY --from=builder /mielin/target/release/mielin-cli /usr/local/bin/mielinctl

# Copy source for development
COPY --from=builder /mielin .

# Set up non-root user
RUN useradd -m -u 1000 mielin && \
    chown -R mielin:mielin /mielin

USER mielin

# Default command
CMD ["bash"]

# Labels
LABEL org.opencontainers.image.title="MielinOS"
LABEL org.opencontainers.image.description="Neural Substrate for the ASI Era"
LABEL org.opencontainers.image.version="0.1.0"
LABEL org.opencontainers.image.authors="MielinOS Contributors"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
