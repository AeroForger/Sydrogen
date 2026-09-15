#!/usr/bin/env bash

set -euo pipefail

REPO="AeroForger/Sydrogen"
API="https://api.github.com/repos/$REPO"
TMP_DIR=""

cleanup() {
    if [[ -n "${TMP_DIR:-}" && -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}

trap cleanup EXIT

echo "==> Installing Sydrogen..."

if ! command -v curl >/dev/null 2>&1; then
    echo "Error: curl is required to install Sydrogen."
    exit 1
fi

if ! command -v tar >/dev/null 2>&1; then
    echo "Error: tar is required to install Sydrogen."
    exit 1
fi

# ------------------------------------------------------------
# Find latest Sydrogen tag
#
# Ordering:
#
#   release > beta > alpha
#
# Within the same channel/version:
#
#   alpha-6.2 > alpha-6 > alpha-6-half-2
#
# The same applies to beta-* and release-*.
# ------------------------------------------------------------

echo "==> Fetching Sydrogen tags..."

TAGS="$(
    curl -fsSL "$API/tags?per_page=100" |
        grep '"name"' |
        sed -E 's/.*"name": *"([^"]+)".*/\1/'
)"

if [[ -z "$TAGS" ]]; then
    echo "Error: no Sydrogen tags found."
    exit 1
fi

version_key() {
    local tag="$1"

    local channel
    local major
    local kind
    local minor

    # Channel priority
    case "$tag" in
        release-*)
            channel=3
            ;;
        beta-*)
            channel=2
            ;;
        alpha-*)
            channel=1
            ;;
        *)
            return
            ;;
    esac

    # Remove channel prefix
    local version="${tag#*-}"

    # X.Y
    if [[ "$version" =~ ^([0-9]+)\.([0-9]+)$ ]]; then
        major="${BASH_REMATCH[1]}"
        minor="${BASH_REMATCH[2]}"

        # Highest form
        kind=3

    # X
    elif [[ "$version" =~ ^([0-9]+)$ ]]; then
        major="${BASH_REMATCH[1]}"
        minor=0

        # Middle form
        kind=2

    # X-half-Y
    elif [[ "$version" =~ ^([0-9]+)-half-([0-9]+)$ ]]; then
        major="${BASH_REMATCH[1]}"
        minor="${BASH_REMATCH[2]}"

        # Lowest form
        kind=1

    else
        return
    fi

    printf "%d %010d %d %010d %s\n" \
        "$channel" \
        "$major" \
        "$kind" \
        "$minor" \
        "$tag"
}

LATEST_TAG="$(
    while IFS= read -r tag; do
        version_key "$tag"
    done <<< "$TAGS" |
        sort -k1,1nr -k2,2nr -k3,3nr -k4,4nr |
        head -n1 |
        awk '{print $5}'
)"

if [[ -z "$LATEST_TAG" ]]; then
    echo "Error: couldn't determine the latest valid Sydrogen tag."
    exit 1
fi

echo "==> Latest Sydrogen tag: $LATEST_TAG"

# ------------------------------------------------------------
# Install Rust/Cargo if necessary
# ------------------------------------------------------------

if ! command -v cargo >/dev/null 2>&1; then
    echo "==> Cargo not found."
    echo "==> Installing Rust using rustup..."

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y

    export PATH="$HOME/.cargo/bin:$PATH"

    if ! command -v cargo >/dev/null 2>&1; then
        echo "Error: Cargo installation failed."
        exit 1
    fi
fi

echo "==> Using $(cargo --version)"

# ------------------------------------------------------------
# Download Sydrogen
# ------------------------------------------------------------

TMP_DIR="$(mktemp -d)"

echo "==> Downloading Sydrogen $LATEST_TAG..."

curl -fsSL \
    "https://github.com/$REPO/archive/refs/tags/$LATEST_TAG.tar.gz" \
    -o "$TMP_DIR/sydrogen.tar.gz"

echo "==> Extracting..."

tar -xzf "$TMP_DIR/sydrogen.tar.gz" -C "$TMP_DIR"

SOURCE_DIR="$(
    find "$TMP_DIR" \
        -mindepth 1 \
        -maxdepth 1 \
        -type d \
        -name 'Sydrogen-*' \
        -print \
        -quit
)"

if [[ -z "$SOURCE_DIR" ]]; then
    echo "Error: couldn't find extracted Sydrogen source."
    exit 1
fi

# ------------------------------------------------------------
# Install Furnace
# ------------------------------------------------------------

echo "==> Building and installing Furnace..."

cd "$SOURCE_DIR"

cargo install --path .

# ------------------------------------------------------------
# Done
# ------------------------------------------------------------

echo
echo "==> Sydrogen $LATEST_TAG installed successfully."
echo "==> Run:"
echo
echo "    furnace --help"
echo
