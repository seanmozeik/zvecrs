#!/usr/bin/env bash
# Builds zvec prebuilt artifacts for linux/amd64 and linux/arm64 and pushes
# a multi-arch image to GHCR. Run from anywhere in the repo.
#
# Prerequisites:
#   docker login ghcr.io -u <github-username>
#   docker buildx create --use   (if no multi-arch builder exists)
#
# Usage:
#   ./scripts/make-prebuilt.sh
#
# Note: linux/amd64 builds via QEMU on Apple Silicon — expect ~60-90 min.
# linux/arm64 builds natively on Apple Silicon — expect ~20-30 min.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION="$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
IMAGE="ghcr.io/seanmozeik/zvec-prebuilt:${VERSION}"

echo "Building $IMAGE for linux/amd64 + linux/arm64..."
echo "This will take a while on first run."
echo ""

# Ensure a multi-arch buildx builder exists
if ! docker buildx inspect multiarch-builder &>/dev/null; then
    echo "Creating multi-arch buildx builder..."
    docker buildx create --name multiarch-builder --driver docker-container --use
fi

docker buildx build \
    --builder multiarch-builder \
    --platform linux/amd64,linux/arm64 \
    --tag "$IMAGE" \
    --push \
    -f "$SCRIPT_DIR/Dockerfile.artifacts" \
    "$REPO_ROOT"

echo ""
echo "Pushed $IMAGE"
echo ""
echo "Use in ultraclaw Dockerfile:"
echo ""
echo "  FROM $IMAGE AS zvec-prebuilt"
echo "  FROM bitnami/minideb:bookworm AS builder"
echo "  COPY --from=zvec-prebuilt /prebuilt /zvec-prebuilt"
echo "  ENV ZVEC_PREBUILT_DIR=/zvec-prebuilt"
echo "  RUN cargo build --release --features zvec"
