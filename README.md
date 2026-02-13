# zvec-rust-bindings

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Rust bindings for [zvec](https://github.com/alibaba/zvec), an open-source in-process vector database built on Alibaba's Proxima engine.

## Features

- Full API coverage matching the Python bindings
- Safe Rust API with proper error handling (`Result<T, Error>`)
- Support for dense and sparse vectors
- HNSW, IVF, and FLAT index types
- Static linking for easy deployment
- Optional thread-safe API via `sync` feature

## Installation

### Prerequisites

**System Requirements:**
- Linux x86_64 or macOS ARM64
- CMake 3.13+
- C++17 compiler (GCC 9+ or Clang 10+)
- Rust 1.70+

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential cmake git pkg-config liblz4-dev
```

**macOS (Homebrew):**
```bash
brew install cmake git lz4
```

### Build

```bash
# Clone with submodules
git clone --recursive https://github.com/your-org/zvec-rust-bindings.git
cd zvec-rust-bindings

# Build (automatically builds zvec C++ and C wrapper)
cargo build --release
```

The first build compiles zvec from source (~5-15 minutes). Subsequent builds are fast.

> 📖 **Need help?** See the [Build Guide](docs/BUILD.md) for detailed instructions and troubleshooting.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
zvec = "0.1"
```

### Basic Example

```rust
use zvec::{create_and_open, CollectionSchema, Doc, VectorQuery, VectorSchema};

fn main() -> zvec::Result<()> {
    // Create schema with a vector field
    let mut schema = CollectionSchema::new("example");
    schema.add_field(VectorSchema::fp32("embedding", 128).into())?;
    
    // Create and open collection
    let collection = create_and_open("./my_vectors", schema)?;
    
    // Insert documents
    let mut doc = Doc::id("doc_1");
    doc.set_vector("embedding", &[0.1, 0.2, 0.3, /* ... */])?;
    collection.insert(&[doc])?;
    
    // Search
    let query = VectorQuery::new("embedding")
        .topk(10)
        .vector(&[0.4, 0.3, 0.2, /* ... */])?;
    
    let results = collection.query(query)?;
    for doc in results.iter() {
        println!("score={:.4}", doc.score());
    }
    
    Ok(())
}
```

### Thread-Safe Usage (Feature: `sync`)

For multi-threaded applications, enable the `sync` feature:

```toml
[dependencies]
zvec = { version = "0.1", features = ["sync"] }
```

```rust
use zvec::{create_and_open_shared, SharedCollection, VectorQuery, VectorSchema, CollectionSchema, Doc};

fn main() -> zvec::Result<()> {
    let mut schema = CollectionSchema::new("example");
    schema.add_field(VectorSchema::fp32("embedding", 128).into())?;
    
    let collection = create_and_open_shared("./my_vectors", schema)?;
    
    // Clone for sharing between threads (cheap - just Arc clone)
    let c1 = collection.clone();
    let c2 = collection.clone();
    
    // Thread 1: concurrent reads
    std::thread::spawn(move || {
        let query = VectorQuery::new("embedding").topk(10).vector(&[0.1, 0.2, 0.3, 0.4]).unwrap();
        let results = c1.query(query).unwrap();
    });
    
    // Thread 2: writes are exclusive
    std::thread::spawn(move || {
        let mut doc = Doc::id("doc_1");
        doc.set_vector("embedding", &[0.1, 0.2, 0.3, 0.4]).unwrap();
        c2.insert(&[doc]).unwrap();
    });
    
    Ok(())
}
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `sync` | Enables `SharedCollection` for thread-safe multi-threaded access |
| `static` | Statically links the zvec C++ library |

## API Coverage

### Collection Operations
- ✅ `create_and_open` - Create and open a new collection
- ✅ `open` - Open an existing collection
- ✅ `flush` - Flush data to disk
- ✅ `destroy` - Delete collection storage

### DML Operations
- ✅ `insert` - Insert documents
- ✅ `upsert` - Insert or update documents
- ✅ `update` - Update existing documents
- ✅ `delete` - Delete documents by primary key
- ✅ `delete_by_filter` - Delete documents matching a filter

### DQL Operations
- ✅ `query` - Vector similarity search
- ✅ `group_by_query` - Grouped vector search
- ✅ `fetch` - Fetch documents by primary key

### DDL Operations
- ✅ `create_index` - Create an index on a column
- ✅ `drop_index` - Drop an index
- ✅ `optimize` - Optimize the collection

### Index Types
- ✅ HNSW (Hierarchical Navigable Small World)
- ✅ IVF (Inverted File)
- ✅ FLAT (Brute Force)
- ✅ INVERT (Inverted Index)

### Data Types
- ✅ Scalar types (bool, int32, int64, float, double, string)
- ✅ Dense vectors (fp16, fp32, fp64, int4, int8, int16)
- ✅ Sparse vectors (fp16, fp32)

## Project Structure

```
zvec-rust-bindings/
├── vendor/zvec/         # Git submodule (zvec C++ library)
├── zvec-c-wrapper/      # C API wrapper for zvec C++ library
│   ├── include/         # C header files
│   └── src/             # C++ implementation
├── zvec-sys/            # Raw FFI bindings (auto-generated by bindgen)
├── zvec/                # Idiomatic Rust API
│   ├── src/             # Library source
│   ├── examples/        # Usage examples
│   └── tests/           # Integration tests
├── docs/                # Documentation
└── scripts/             # Build and utility scripts
```

## Documentation

- [Build Guide](docs/BUILD.md) - Detailed build instructions and troubleshooting
- [API Documentation](https://docs.rs/zvec) - Rust API docs

## Development

```bash
# Run tests
cargo test

# Run tests with sync feature
cargo test --features sync

# Run examples
cargo run --example basic
cargo run --example crud
cargo run --example search

# Check code
cargo clippy --all-features
cargo fmt --check

# Build docs
cargo doc --open

# Run coverage
./scripts/coverage.sh
```

## License

Apache-2.0

## Acknowledgments

- [zvec](https://github.com/alibaba/zvec) - The underlying vector database
- [Alibaba Proxima](https://github.com/alibaba/proxima) - The core vector search engine
