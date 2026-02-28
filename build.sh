#!/usr/bin/env bash
#
# Tunnels Protocol - Release Build Script
#
# This script builds release binaries for tunnels-node and tunnels-keygen,
# generates SHA-256 checksums, and prepares artifacts for distribution.
#
# Usage:
#   ./build.sh              # Build all binaries
#   ./build.sh clean        # Clean build artifacts
#   ./build.sh check        # Run tests and clippy before building
#
# Requirements:
#   - Rust 1.75 or later (rustup install 1.75)
#   - RocksDB development libraries (see docs/running-a-node.md)
#   - macOS: brew install rocksdb
#   - Ubuntu: apt install librocksdb-dev
#   - Fedora: dnf install rocksdb-devel

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${SCRIPT_DIR}/target/release"
DIST_DIR="${SCRIPT_DIR}/dist"
BINARIES=("tunnels-node" "tunnels-keygen")

# Print with color
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Check prerequisites
check_prerequisites() {
    info "Checking prerequisites..."

    # Check Rust version
    if ! command -v rustc &> /dev/null; then
        error "Rust is not installed. Install from https://rustup.rs"
    fi

    RUST_VERSION=$(rustc --version | cut -d' ' -f2)
    REQUIRED_VERSION="1.75.0"
    if [[ "$(printf '%s\n' "$REQUIRED_VERSION" "$RUST_VERSION" | sort -V | head -n1)" != "$REQUIRED_VERSION" ]]; then
        error "Rust $REQUIRED_VERSION or later required (found $RUST_VERSION)"
    fi
    info "Rust version: $RUST_VERSION"

    # Check cargo
    if ! command -v cargo &> /dev/null; then
        error "Cargo not found"
    fi

    # Check for sha256sum or shasum
    if command -v sha256sum &> /dev/null; then
        SHA256_CMD="sha256sum"
    elif command -v shasum &> /dev/null; then
        SHA256_CMD="shasum -a 256"
    else
        error "sha256sum or shasum required for checksums"
    fi
    info "Using $SHA256_CMD for checksums"
}

# Clean build artifacts
clean() {
    info "Cleaning build artifacts..."
    cargo clean
    rm -rf "${DIST_DIR}"
    info "Clean complete"
}

# Run tests and clippy
check() {
    info "Running clippy..."
    cargo clippy --workspace -- -D warnings

    info "Running tests..."
    cargo test --workspace

    info "All checks passed"
}

# Build release binaries
build_release() {
    info "Building release binaries..."

    # Build all binaries in release mode
    cargo build --release -p tunnels-node -p tunnels-keygen

    info "Build complete"
}

# Create distribution directory with binaries and checksums
create_dist() {
    info "Creating distribution..."

    # Create dist directory
    mkdir -p "${DIST_DIR}"

    # Detect platform
    PLATFORM="unknown"
    case "$(uname -s)" in
        Linux*)  PLATFORM="linux";;
        Darwin*) PLATFORM="macos";;
        *)       PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')";;
    esac

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64)  ARCH="x86_64";;
        arm64)   ARCH="aarch64";;
        aarch64) ARCH="aarch64";;
    esac

    SUFFIX="${PLATFORM}-${ARCH}"
    info "Platform: ${SUFFIX}"

    # Copy binaries
    for binary in "${BINARIES[@]}"; do
        if [[ -f "${TARGET_DIR}/${binary}" ]]; then
            cp "${TARGET_DIR}/${binary}" "${DIST_DIR}/${binary}-${SUFFIX}"
            info "Copied ${binary}"
        else
            warn "Binary not found: ${binary}"
        fi
    done

    # Generate checksums
    info "Generating checksums..."
    cd "${DIST_DIR}"
    $SHA256_CMD * > checksums.txt
    cd "${SCRIPT_DIR}"

    info "Distribution created in ${DIST_DIR}"
    echo ""
    echo "=== Build Summary ==="
    echo "Binaries:"
    ls -lh "${DIST_DIR}"/*.* 2>/dev/null || echo "  (none)"
    echo ""
    echo "Checksums:"
    cat "${DIST_DIR}/checksums.txt"
}

# Show build info
show_info() {
    echo ""
    echo "=== Tunnels Protocol Build ==="
    echo "Rust version:  $(rustc --version)"
    echo "Cargo version: $(cargo --version)"
    echo "Target:        $(rustc -vV | grep host | cut -d' ' -f2)"
    echo ""
}

# Main
main() {
    cd "${SCRIPT_DIR}"

    case "${1:-build}" in
        clean)
            clean
            ;;
        check)
            check_prerequisites
            check
            ;;
        build|"")
            check_prerequisites
            show_info
            build_release
            create_dist
            ;;
        help|--help|-h)
            echo "Usage: $0 [command]"
            echo ""
            echo "Commands:"
            echo "  build   Build release binaries (default)"
            echo "  clean   Clean build artifacts"
            echo "  check   Run tests and clippy"
            echo "  help    Show this help"
            ;;
        *)
            error "Unknown command: $1"
            ;;
    esac
}

main "$@"
