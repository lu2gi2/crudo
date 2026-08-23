#!/bin/bash
set -e

# Build the release binary
cargo build --release

# Link to ~/.local/bin
mkdir -p "$HOME/.local/bin"
ln -sf "$(pwd)/target/release/crudo" "$HOME/.local/bin/crudo"

echo "✓ Crudo installed to ~/.local/bin/crudo"
