#!/bin/bash
set -e

echo "Building agent-router-mcp for all platforms..."
echo ""

# Check if cargo-zigbuild is installed
if ! command -v cargo-zigbuild &> /dev/null; then
    echo "cargo-zigbuild not found. Installing..."
    cargo install cargo-zigbuild
fi

# Check if mingw-w64 is installed (required for Windows builds)
if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    echo "Warning: mingw-w64 not found. Windows builds will fail."
    echo "Install with: brew install mingw-w64"
    echo ""
fi

# Create dist directory
mkdir -p dist

# Define targets
declare -a targets=(
    "x86_64-unknown-linux-gnu:agent-router-mcp-linux-amd64"
    "aarch64-unknown-linux-gnu:agent-router-mcp-linux-arm64"
    "x86_64-apple-darwin:agent-router-mcp-macos-intel"
    "aarch64-apple-darwin:agent-router-mcp-macos-silicon"
    "x86_64-pc-windows-gnu:agent-router-mcp-windows-amd64.exe"
    "aarch64-pc-windows-gnullvm:agent-router-mcp-windows-arm64.exe"
)

# Install all required Rust targets
echo "Installing Rust targets..."
for target_info in "${targets[@]}"; do
    IFS=':' read -r target _ <<< "$target_info"
    rustup target add "$target" || true
done
echo ""

# Build for each target
for target_info in "${targets[@]}"; do
    IFS=':' read -r target output_name <<< "$target_info"

    echo "Building for $target..."
    cargo zigbuild --release --target "$target"

    # Determine source binary name
    if [[ "$target" == *"windows"* ]]; then
        src_binary="target/$target/release/agent-router-mcp.exe"
    else
        src_binary="target/$target/release/agent-router-mcp"
    fi

    # Copy to dist with platform-specific name
    cp "$src_binary" "dist/$output_name"
    echo "✓ Built: dist/$output_name"
    echo ""
done

echo "All builds complete!"
echo ""
echo "Binaries created:"
ls -lh dist/

echo ""
echo "Creating config file archives..."

# Create archives for each platform
# Windows: zip
if command -v zip &> /dev/null; then
    cd config
    zip -r ../dist/agent-router-mcp-config.zip agents.json llm-tags.json rules.json
    cd ..
    echo "✓ Created: dist/agent-router-mcp-config.zip (Windows)"
else
    echo "Warning: zip not found, skipping .zip archive"
fi

# macOS/Linux: tar.gz
if command -v tar &> /dev/null; then
    tar -czf dist/agent-router-mcp-config.tar.gz -C config agents.json llm-tags.json rules.json
    echo "✓ Created: dist/agent-router-mcp-config.tar.gz (macOS/Linux)"
else
    echo "Warning: tar not found, skipping .tar.gz archive"
fi

echo ""
echo "All artifacts created:"
ls -lh dist/
