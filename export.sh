#!/bin/bash
# Redblue Portable Export Script
# Usage: ./export.sh

set -e

echo "Creating Redblue portable package..."

# Create export directory
EXPORT_DIR="redblue-portable-$(date +%Y%m%d)"
mkdir -p "$EXPORT_DIR"

# Copy essential files
echo "Copying files..."
cp -r src "$EXPORT_DIR/"
cp -r tests "$EXPORT_DIR/"
cp -r examples "$EXPORT_DIR/"
cp -r docs "$EXPORT_DIR/"
cp -r tooling "$EXPORT_DIR/"
cp Cargo.toml "$EXPORT_DIR/"
cp README.md "$EXPORT_DIR/"
cp LICENSE "$EXPORT_DIR/"
cp AGENTS.md "$EXPORT_DIR/"
cp SPEC.md "$EXPORT_DIR/"
cp PHILOSOPHY.md "$EXPORT_DIR/"
cp ROADMAP.md "$EXPORT_DIR/"
cp HANDOUT.md "$EXPORT_DIR/"
cp CODE_OF_CONDUCT.md "$EXPORT_DIR/"
cp CONTRIBUTING.md "$EXPORT_DIR/"

# Copy GitHub config (optional)
if [ -d ".github" ]; then
    cp -r .github "$EXPORT_DIR/"
fi

# Create install script for Unix
cat > "$EXPORT_DIR/install.sh" << 'EOF'
#!/bin/bash
# Redblue Installation Script

echo "Installing Redblue..."

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "Rust not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source ~/.cargo/env
fi

# Build
echo "Building Redblue..."
cargo build --release

# Install
echo "Installing to ~/.cargo/bin..."
mkdir -p ~/.cargo/bin
cp target/release/redblue ~/.cargo/bin/ 2>/dev/null || cp target/release/redblue.exe ~/.cargo/bin/

# Add to PATH
if ! echo $PATH | grep -q "$HOME/.cargo/bin"; then
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
    export PATH="$HOME/.cargo/bin:$PATH"
fi

echo "Installation complete!"
echo "Run 'rb' or 'redblue' to start."
EOF
chmod +x "$EXPORT_DIR/install.sh"

# Create install script for Windows
cat > "$EXPORT_DIR/install.bat" << 'EOF'
@echo off
echo Installing Redblue...

REM Check for Rust
where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo Rust not found. Installing...
    powershell -Command "irm rustup.rs | iex"
)

REM Build
echo Building Redblue...
cargo build --release

REM Create bin directory
if not exist "%USERPROFILE%\.cargo\bin" mkdir "%USERPROFILE%\.cargo\bin"

REM Copy executable
copy target\release\redblue.exe "%USERPROFILE%\.cargo\bin\"

REM Add to PATH
setx PATH "%PATH%;%USERPROFILE%\.cargo\bin"

echo Installation complete!
echo Run 'rb' or 'redblue' to start.
pause
EOF

# Create README for export
cat > "$EXPORT_DIR/README-EXPORT.md" << 'EOF'
# Redblue Portable Package

This package contains the complete Redblue programming language source code.

## Quick Start

### Linux/macOS
```bash
./install.sh
rb
```

### Windows
```batch
install.bat
rb
```

## Manual Installation

1. Install Rust: https://rustup.rs
2. Build:
   ```bash
   cargo build --release
   ```
3. Run:
   ```bash
   cargo run
   ```

## Contents

- `src/` - Source code
- `tests/` - Test suite
- `examples/` - Example programs
- `docs/` - Documentation
- `HANDOUT.md` - Quick reference guide
- `AGENTS.md` - Developer guide

## License

MIT License - See LICENSE file
EOF

# Create archive
echo "Creating archive..."
cd ..
tar -czvf "${EXPORT_DIR}.tar.gz" "$EXPORT_DIR"

# Create zip for Windows
if command -v zip &> /dev/null; then
    zip -r "${EXPORT_DIR}.zip" "$EXPORT_DIR"
fi

echo ""
echo "Export complete!"
echo ""
echo "Files created:"
ls -lh "${EXPORT_DIR}"*
echo ""
echo "Transfer these files to another computer:"
echo "  - ${EXPORT_DIR}.tar.gz (Linux/macOS)"
echo "  - ${EXPORT_DIR}/ (entire folder)"
EOF
