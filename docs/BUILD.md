# Building zvec Rust Bindings

This guide covers building the zvec Rust bindings from source. The build process is fully automated via Cargo's `build.rs` system.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Build Process](#build-process)
- [Configuration](#configuration)
- [Troubleshooting](#troubleshooting)
- [Manual Build](#manual-build-advanced)
- [Development](#development)

## Prerequisites

### System Requirements

| Requirement | Minimum Version |
|-------------|-----------------|
| OS | Linux x86_64 or macOS ARM64 |
| CMake | 3.13+ |
| C++ Compiler | GCC 9+ or Clang 10+ (C++17 support) |
| Rust | 1.70+ |
| Git | 2.0+ |

### System Dependencies

The zvec library has several third-party dependencies. You have two options:

#### Option A: Build from zvec's bundled sources (recommended)

zvec includes all dependencies in its `thirdparty/` directory. These are built automatically.

You still need a few system packages:

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    cmake \
    git \
    pkg-config \
    liblz4-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install -y \
    gcc-c++ \
    cmake \
    git \
    pkg-config \
    lz4-devel
```

**macOS (Homebrew):**
```bash
brew install cmake git lz4
```

#### Option B: Use system packages

If you prefer to use system-installed dependencies (faster rebuilds):

**Ubuntu/Debian:**
```bash
sudo apt-get install -y \
    build-essential cmake git pkg-config \
    librocksdb-dev \
    libprotobuf-dev protobuf-compiler \
    liblz4-dev \
    libgflags-dev libgoogle-glog-dev \
    libyaml-cpp-dev \
    libre2-dev
```

**macOS:**
```bash
brew install cmake git rocksdb protobuf lz4 gflags glog yaml-cpp re2
```

## Quick Start

```bash
# Clone with submodules
git clone --recursive https://github.com/your-org/zvec-rust-bindings.git
cd zvec-rust-bindings

# Build everything (this will take a while on first build)
cargo build --release
```

The first build compiles:
1. zvec C++ library (~5-15 minutes depending on your machine)
2. C wrapper layer (~30 seconds)
3. Rust bindings (via bindgen)
4. Rust crate

Subsequent builds are fast - only changed files are recompiled.

## Build Process

### Automated Build Flow

```
cargo build
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  zvec-sys/build.rs                                              │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 1. Check: Does vendor/zvec/lib/*.a exist?               │   │
│  │    NO → Run cmake + make to build zvec                  │   │
│  │                                                          │   │
│  │ 2. Check: Does zvec-c-wrapper/build/*.a exist?          │   │
│  │    NO → Run cmake + make to build C wrapper             │   │
│  │                                                          │   │
│  │ 3. Run bindgen to generate Rust FFI from zvec_c.h       │   │
│  │                                                          │   │
│  │ 4. Emit cargo:rustc-link-lib directives                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Rust Compiler + Linker                                         │
│                                                                 │
│  Links all static libraries into final binary:                  │
│  - libzvec_c_wrapper.a (C wrapper)                              │
│  - libzvec_*.a (zvec components)                                │
│  - libcore_*.a (core index implementations)                     │
│  - Third-party libs (rocksdb, protobuf, arrow, etc.)            │
└─────────────────────────────────────────────────────────────────┘
```

### What Gets Built

| Component | Location | Description |
|-----------|----------|-------------|
| zvec C++ | `vendor/zvec/lib/*.a` | Core vector database library |
| C wrapper | `zvec-c-wrapper/build/libzvec_c_wrapper.a` | C API shim |
| Rust FFI | `target/debug/build/zvec-sys-*/out/bindings.rs` | Auto-generated bindings |
| zvec crate | `target/debug/libzvec*.rlib` | Rust library |

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZVEC_BUILD_TYPE` | `Release` | CMake build type (`Debug`, `Release`, `RelWithDebInfo`) |
| `ZVEC_BUILD_PARALLEL` | CPU count | Number of parallel make jobs |

### Examples

```bash
# Debug build (faster compile, slower runtime)
cargo build

# Release build (slower compile, faster runtime)
cargo build --release

# Limit parallel jobs (useful for low-memory systems)
ZVEC_BUILD_PARALLEL=2 cargo build

# Debug build for zvec C++ code
ZVEC_BUILD_TYPE=Debug cargo build
```

### Clean Build

```bash
# Clean only Rust artifacts
cargo clean

# Clean everything (including C++ builds)
cargo clean
rm -rf vendor/zvec/build
rm -rf zvec-c-wrapper/build
```

## Feature Flags

The `zvec` crate supports the following features:

| Feature | Description |
|---------|-------------|
| `sync` | Enables `SharedCollection` for thread-safe multi-threaded access |
| `static` | Statically links the zvec C++ library |

### Using Features

```toml
# In Cargo.toml
[dependencies]
zvec = { version = "0.1", features = ["sync"] }
```

```bash
# Build with features
cargo build --features sync
cargo test --features sync
```

## Troubleshooting

### "cmake: command not found"

Install CMake:

```bash
# Ubuntu/Debian
sudo apt-get install cmake

# macOS
brew install cmake
```

### Linker Errors: "undefined reference to ..."

This usually means zvec's third-party dependencies weren't built correctly.

**Solution 1: Clean and rebuild**
```bash
cargo clean
rm -rf vendor/zvec/build vendor/zvec/lib
cargo build
```

**Solution 2: Install system dependencies**
```bash
# Ubuntu/Debian
sudo apt-get install librocksdb-dev libprotobuf-dev liblz4-dev
```

### Out of Memory During Build

zvec's C++ compilation can use significant memory. Reduce parallel jobs:

```bash
ZVEC_BUILD_PARALLEL=2 cargo build
```

### "fatal: not a git repository" during submodule init

If you downloaded the source as a zip/tarball instead of cloning:

```bash
git init
git submodule add https://github.com/alibaba/zvec.git vendor/zvec
cargo build
```

### Protobuf Errors

If you see protobuf-related errors:

```bash
# Install protobuf compiler
sudo apt-get install protobuf-compiler libprotobuf-dev

# Clean and rebuild
rm -rf vendor/zvec/build
cargo build
```

### RocksDB Errors

If you see RocksDB-related linking errors:

```bash
# Option 1: Install system rocksdb
sudo apt-get install librocksdb-dev

# Option 2: Ensure zvec builds its bundled version
rm -rf vendor/zvec/build vendor/zvec/lib
rm -rf vendor/zvec/thirdparty/rocksdb/rocksdb-*/build*
cargo build
```

## Manual Build (Advanced)

For more control over the build process, you can build each component manually.

### Step 1: Build zvec C++ Library

```bash
cd vendor/zvec
mkdir -p build && cd build

cmake \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_PYTHON_BINDINGS=OFF \
    -DBUILD_TOOLS=OFF \
    ..

make -j$(nproc)
# Libraries are output to ../lib/
```

### Step 2: Build C Wrapper

```bash
cd ../../zvec-c-wrapper
mkdir -p build && cd build

cmake -DZVEC_SRC_DIR=../vendor/zvec ..
make -j$(nproc)
# Library is output to ./libzvec_c_wrapper.a
```

### Step 3: Build Rust

```bash
cd ../..
cargo build
```

## Development

### Regenerate FFI Bindings

Bindings are auto-generated by bindgen. To force regeneration:

```bash
rm -rf target/debug/build/zvec-sys-*
cargo build
```

### Run Tests

```bash
# Run all tests
cargo test

# Run tests with sync feature
cargo test --features sync
```

### Run Examples

```bash
cargo run --example basic
cargo run --example crud
cargo run --example search
cargo run --example indexes
cargo run --example sparse
```

### Check Code

```bash
# Check with all features
cargo clippy --all-features
cargo fmt --check
```

### Build Documentation

```bash
cargo doc --open
```

## Platform-Specific Notes

### Linux

- Tested on Ubuntu 20.04+ and Fedora 36+
- Requires glibc 2.31+

### macOS

- Tested on macOS 13+ (Ventura) with Apple Silicon
- Intel Macs may work but are not officially supported
- Xcode Command Line Tools required: `xcode-select --install`

### Windows

- Currently not supported
- Would require porting the C wrapper to MSVC/MinGW

## Getting Help

- [GitHub Issues](https://github.com/your-org/zvec-rust-bindings/issues)
- [zvec Documentation](https://zvec.org/en/docs/)
- [zvec Discord](https://discord.gg/rKddFBBu9z)
