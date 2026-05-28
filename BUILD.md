# Build Instructions

## Native Build

### macOS

```bash
# Build debug
cargo build

# Build release
cargo build --release

# Run
cargo run --release

# Create macOS .app bundle
./scripts/bundle_macos.sh
```

**Binary location:** `target/release/netherize_editor`

### Linux

```bash
# Install dependencies first
# Ubuntu/Debian:
sudo apt install build-essential pkg-config libfontconfig1-dev libfreetype6-dev

# Fedora/RHEL:
sudo dnf install gcc pkg-config fontconfig-devel freetype-devel

# Arch:
sudo pacman -S base-devel fontconfig freetype2

# Build
cargo build --release

# Create Linux bundle
./scripts/bundle_linux.sh
```

**Binary location:** `target/release/netherize_editor`

### Windows

```bash
# Build
cargo build --release

# Create Windows bundle (cross-compile from macOS)
./scripts/bundle_windows.sh
```

**Binary location:** `target/release/netherize_editor.exe`

## Cross-Compilation

### Linux from macOS

**Requirements:**
- Docker Desktop installed and running
- `cross` tool: `cargo install cross --git https://github.com/cross-rs/cross`

```bash
# Build Linux binary from macOS
cross build --release --target x86_64-unknown-linux-gnu

# Or use the bundle script (does everything)
./scripts/bundle_linux.sh
```

**Binary location:** `target/x86_64-unknown-linux-gnu/release/netherize_editor`

**First build:** Takes 10-15 minutes (downloads Docker image + compiles all dependencies)  
**Subsequent builds:** 2-3 minutes (cached)

### Windows from macOS

**Requirements:**
- `cargo-xwin`: `cargo install cargo-xwin`

```bash
# Build Windows binary from macOS
cargo xwin build --release --target x86_64-pc-windows-msvc

# Or use the bundle script
./scripts/bundle_windows.sh
```

## Build Profiles

### Release (default)
```bash
cargo build --release
```
- Full optimization (`opt-level = 3`)
- Thin LTO
- Single codegen unit
- Stripped debug info
- Panic = abort

### Profiling
```bash
cargo build --profile profiling
```
- Same as release but keeps debug symbols
- Use with flamegraph: `./scripts/profile_flamegraph.sh`

### Debug
```bash
cargo build
```
- Fast compilation
- No optimization
- Full debug info

## Distribution Packages

All bundle scripts create ready-to-distribute packages:

```bash
# macOS: Creates .app bundle + .zip
./scripts/bundle_macos.sh v1.0.4-alpha
# Output: target/Netherize-v1.0.4-alpha-macos.zip

# Linux: Creates tarball with install script
./scripts/bundle_linux.sh v1.0.4-alpha
# Output: dist/netherize-editor-v1.0.4-alpha-linux-x86_64.tar.gz

# Windows: Creates folder with .exe + launcher
./scripts/bundle_windows.sh
# Output: dist/windows/
```

## Troubleshooting

### macOS: "cannot find -lSystem"
```bash
xcode-select --install
```

### Linux: "fontconfig not found"
```bash
# Install fontconfig development headers
sudo apt install libfontconfig1-dev  # Ubuntu/Debian
sudo dnf install fontconfig-devel    # Fedora
sudo pacman -S fontconfig            # Arch
```

### Cross-compilation: "GLIBC version not found"
The `Cross.toml` file is already configured with the correct Docker image. Make sure Docker Desktop is running.

### Cross-compilation: "Docker not found"
```bash
# macOS
brew install --cask docker
# Then open Docker Desktop and wait for it to start
```

## CI/CD

For automated builds on GitHub Actions, see `.github/workflows/` (to be added).

## Binary Size

- **Debug:** ~150-200 MB (with debug symbols)
- **Release:** ~15-25 MB (stripped)
- **Release (compressed):** ~5-8 MB (in tarball/zip)

## Performance Benchmarks

```bash
# Run benchmarks
cargo bench

# Run E2E performance test
./scripts/run_perf_baseline.sh

# Profile with flamegraph
./scripts/profile_flamegraph.sh
```
