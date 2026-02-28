# zvec HNSW Vector Store Integration

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the brute-force `InMemoryVectorStore` (`HashMap` with O(n) cosine scan) with a zvec-backed HNSW vector store, gaining O(log n) approximate nearest-neighbor search and on-disk persistence.

**Architecture:** The existing `VectorStore` trait (`memory/vector/mod.rs:55-88`) is the abstraction boundary. We evolve the trait for interior mutability (`&self` instead of `&mut self`), add a `get_vectors` method for dedup, create a `ZvecVectorStore` implementation behind a cargo feature flag, and refactor all pipeline consumers away from raw `HashMap` access to use the trait.

**zvec-rust-binding:** Fork at `~/dev/zvecrs` (GitHub: `seanmozeik/zvecrs`). Needs patches applied from `~/Downloads/zvec-rust-binding/PATCHES.md` before use.

---

## Prerequisites

1. Apply patches to `~/dev/zvecrs` (Phase 0 below), push to GitHub.
2. Verify: `cd ~/dev/zvecrs && cargo test --workspace` passes.

---

## Phase 0: Patch zvecrs Fork

**Repo:** `~/dev/zvecrs` (GitHub: `seanmozeik/zvecrs`)

Reference: `~/Downloads/zvec-rust-binding/PATCHES.md` for full context.

### Task 0.1: Clone vendor and patch ANTLR CMake

```bash
cd ~/dev/zvecrs
git clone --branch v0.2.0 --depth 1 https://github.com/alibaba/zvec vendor/zvec
```

**File:** `vendor/zvec/thirdparty/antlr/antlr4/runtime/Cpp/CMakeLists.txt`

Change 4 policy settings from `OLD` to `NEW`:
- Line 31: `CMP0054` OLD → NEW
- Line 32: `CMP0045` OLD → NEW
- Line 33: `CMP0042` OLD → NEW
- Line 38: `CMP0059` OLD → NEW
- Line 39: `CMP0054` OLD → NEW (duplicate entry)

### Task 0.2: Fix AddColumn API mismatch

**File:** `zvec-sys/zvec-c-wrapper/src/collection.cpp:236`

Remove the extra argument `column_schema->ptr->name(),` from the `AddColumn` call, leaving:
```cpp
AddColumn(column_schema->ptr, expression, opts)
```

### Task 0.3: Fix dangling pointer in doc string returns

**File:** `zvec-sys/zvec-c-wrapper/include/zvec_c_internal.h`

Add after line 60 (inside the doc struct):
```cpp
mutable std::string string_cache;
```

**File:** `zvec-sys/zvec-c-wrapper/src/doc.cpp`

Line 43 — `zvec_doc_pk`:
```cpp
// OLD:
return doc->ptr->pk().c_str();

// NEW:
doc->string_cache = doc->ptr->pk();
return doc->string_cache.c_str();
```

Line 328 — `zvec_doc_get_string`:
```cpp
// OLD:
*out_value = result.value().c_str();

// NEW:
doc->string_cache = result.value();
*out_value = doc->string_cache.c_str();
```

### Task 0.4: Fix build.rs (rerun triggers, cmake policy, macOS linker)

**File:** `zvec-sys/build.rs`

**A) Add rerun-if-changed triggers** (after existing rerun lines, ~line 51):
```rust
println!("cargo:rerun-if-changed=zvec-c-wrapper/CMakeLists.txt");
println!("cargo:rerun-if-changed=zvec-c-wrapper/include/zvec_c.h");
println!("cargo:rerun-if-changed=zvec-c-wrapper/include/zvec_c_internal.h");
println!("cargo:rerun-if-changed=zvec-c-wrapper/src");
```

**B) Always rebuild wrapper** (~lines 73-85): Remove the `if !wrapper_built.exists()` conditional. Always call `build_c_wrapper(...)`.

**C) Add cmake policy flag** (~line 98): Add to the cmake configure args:
```rust
"-DCMAKE_POLICY_VERSION_MINIMUM=3.5".to_string(),
```

**D) macOS linker fix** (~lines 290-294): Replace unconditional `stdc++` linking:
```rust
// OLD:
println!("cargo:rustc-link-lib=stdc++");
println!("cargo:rustc-link-lib=pthread");
println!("cargo:rustc-link-lib=dl");
println!("cargo:rustc-link-lib=m");

// NEW:
if cfg!(target_os = "macos") {
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=m");
} else {
    println!("cargo:rustc-link-lib=stdc++");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=m");
}
```

### Task 0.5: Version bump

**File:** `Cargo.toml` (workspace root)

```toml
version = "0.2.1"  # was "0.2.0"
```

### Task 0.6: Commit and push

```bash
cd ~/dev/zvecrs
git add -A
git commit -m "fix: apply build patches for macOS + modern cmake + dangling pointer"
git push origin main
```

### Verify

```bash
cd ~/dev/zvecrs && cargo test --workspace
```

---

## Phase 1: VectorStore Trait Evolution + Interior Mutability

**File:** `runtime/core-rust/src/memory/vector/mod.rs`

This is the foundation — all subsequent phases depend on it.

### Task 1.1: Evolve VectorStore trait signatures

Change `&mut self` to `&self` on `upsert` and `delete`. Add `get_vectors` method.

**Current trait** (lines 55-88):
```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&mut self, id: String, vector: Vec<f32>) -> Result<(), VectorError>;
    async fn query(&self, vector: Vec<f32>, top_k: usize) -> Result<Vec<VectorHit>, VectorError>;
    async fn delete(&mut self, id: &str) -> Result<(), VectorError>;
    async fn flush(&self) -> Result<(), VectorError>;
}
```

**New trait:**
```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, id: String, vector: Vec<f32>) -> Result<(), VectorError>;
    async fn query(&self, vector: Vec<f32>, top_k: usize) -> Result<Vec<VectorHit>, VectorError>;
    async fn delete(&self, id: &str) -> Result<(), VectorError>;
    async fn flush(&self) -> Result<(), VectorError>;
    /// Fetch stored vectors by entity ID for pairwise comparison (cosine dedup).
    /// IDs not found in the store are silently omitted.
    async fn get_vectors(&self, ids: &[String]) -> Result<HashMap<String, Vec<f32>>, VectorError>;
}
```

**Why `&self`:** zvec's `Collection` uses interior mutability via C++ internal locks. Keeping `&mut self` would force unnecessary external synchronization. The `InMemoryVectorStore` switches to `std::sync::RwLock` for the same pattern.

### Task 1.2: InMemoryVectorStore — interior mutability

**Current struct** (lines 96-101):
```rust
pub struct InMemoryVectorStore {
    vectors: HashMap<String, Vec<f32>>,
    dimensions: usize,
}
```

**New struct:**
```rust
pub struct InMemoryVectorStore {
    vectors: std::sync::RwLock<HashMap<String, Vec<f32>>>,
    dimensions: usize,
}
```

Update `new()` (line 105-110):
```rust
pub fn new(dimensions: usize) -> Self {
    Self {
        vectors: std::sync::RwLock::new(HashMap::new()),
        dimensions,
    }
}
```

**Delete `as_map()`** (lines 112-119) — no longer possible with internal RwLock. This is the key breaking change that forces pipeline refactoring in Phase 2.

Keep `cosine_similarity` as a private helper (lines 121-142) — still used by the brute-force query impl.

### Task 1.3: Update VectorStore impl for InMemoryVectorStore

All methods change from direct `self.vectors.X()` to acquiring locks.

**`upsert`** (was `&mut self`, now `&self`):
```rust
async fn upsert(&self, id: String, vector: Vec<f32>) -> Result<(), VectorError> {
    if vector.len() != self.dimensions {
        return Err(VectorError::DimensionMismatch { ... });
    }
    let mut guard = self.vectors.write()
        .map_err(|e| VectorError::Internal(format!("lock poisoned: {e}")))?;
    guard.insert(id, vector);
    Ok(())
}
```

**`query`** (was `&self`, stays `&self`):
```rust
async fn query(&self, vector: Vec<f32>, top_k: usize) -> Result<Vec<VectorHit>, VectorError> {
    if vector.len() != self.dimensions {
        return Err(VectorError::DimensionMismatch { ... });
    }
    let guard = self.vectors.read()
        .map_err(|e| VectorError::Internal(format!("lock poisoned: {e}")))?;
    // Same brute-force cosine scan logic as before, using `guard.iter()`
    ...
}
```

**`delete`** (was `&mut self`, now `&self`):
```rust
async fn delete(&self, id: &str) -> Result<(), VectorError> {
    let mut guard = self.vectors.write()
        .map_err(|e| VectorError::Internal(format!("lock poisoned: {e}")))?;
    guard.remove(id);
    Ok(())
}
```

**`flush`**: unchanged (no-op).

**`get_vectors`** (new):
```rust
async fn get_vectors(&self, ids: &[String]) -> Result<HashMap<String, Vec<f32>>, VectorError> {
    let guard = self.vectors.read()
        .map_err(|e| VectorError::Internal(format!("lock poisoned: {e}")))?;
    let mut result = HashMap::with_capacity(ids.len());
    for id in ids {
        if let Some(vec) = guard.get(id) {
            result.insert(id.clone(), vec.clone());
        }
    }
    Ok(result)
}
```

### Task 1.4: Update tests

Tests in `mod tests` (lines 197-280): remove `mut` from `let mut store = InMemoryVectorStore::new(3)` since `upsert`/`delete` now take `&self`.

### Verify

```bash
cd runtime/core-rust && cargo test --lib memory::vector -- --nocapture
```

Expected: tests pass but other modules fail to compile (pipeline consumers still reference `as_map()`). That's fine — Phase 2 fixes them.

---

## Phase 2: Pipeline Refactoring (HashMap → &dyn VectorStore)

All three `HashMap<String, Vec<f32>>` consumers switch to `&dyn VectorStore`.

### Task 2.1: Refactor `resolve_conflict` (merger.rs)

**File:** `runtime/core-rust/src/memory/entity/merger.rs`

**Signature change** (line 65-74):

```rust
// OLD:
pub async fn resolve_conflict<S: BuildHasher + Sync>(
    pool: &SqlitePool,
    vectors: &HashMap<String, Vec<f32>, S>,
    graph: &mut PetGraphStore,
    config: &MemoryConfig,
    new_content: &str,
    scope_key: &str,
    embedding: &[f32],
    embedding_model: Option<&str>,
) -> Result<MergeOutcome, sqlx::Error> {

// NEW:
pub async fn resolve_conflict(
    pool: &SqlitePool,
    vectors: &dyn VectorStore,
    graph: &mut PetGraphStore,
    config: &MemoryConfig,
    new_content: &str,
    scope_key: &str,
    embedding: &[f32],
    embedding_model: Option<&str>,
) -> Result<MergeOutcome, sqlx::Error> {
```

**Replace the full scan** (lines 76-82):

```rust
// OLD:
let mut scored: Vec<(String, f64)> = vectors
    .iter()
    .map(|(eid, emb)| (eid.clone(), cosine_similarity(emb, embedding)))
    .collect();
scored.sort_unstable_by(...);

// NEW:
let candidates = vectors
    .query(embedding.to_vec(), TOP_K)
    .await
    .unwrap_or_default();
```

**Replace the iteration** (line 84):

```rust
// OLD:
for (candidate_id, similarity) in scored.iter().take(TOP_K) {

// NEW:
for hit in &candidates {
    let candidate_id = &hit.entity_id;
    let similarity = hit.score;
```

Adjust the body: `*similarity` → `similarity`, `candidate_id.clone()` works the same.

**Import changes** (lines 1-19):
- Remove: `use std::collections::HashMap;` and `use std::hash::BuildHasher;`
- Add: `use crate::memory::vector::VectorStore;`
- Remove: `use crate::memory::retrieval::assembler::cosine_similarity;` (no longer needed)

**Test changes** (lines 156-295):
- Remove: `use std::collections::HashMap;`
- Add: `use crate::memory::vector::InMemoryVectorStore;`
- Change all test bodies from:
  ```rust
  let mut vectors: HashMap<String, Vec<f32>> = HashMap::new();
  vectors.insert("existing-merge".to_owned(), existing_emb);
  resolve_conflict(&pool, &vectors, ...)
  ```
  to:
  ```rust
  let store = InMemoryVectorStore::new(3);
  store.upsert("existing-merge".to_owned(), existing_emb).await.unwrap();
  resolve_conflict(&pool, &store, ...)
  ```

### Task 2.2: Refactor `run_enrich_pipeline` (enrich/mod.rs)

**File:** `runtime/core-rust/src/memory/enrich/mod.rs`

**Signature change** (line 50-58):

```rust
// OLD:
pub async fn run_enrich_pipeline<S: BuildHasher + Sync>(
    pool: &SqlitePool,
    vectors: &HashMap<String, Vec<f32>, S>,
    ...

// NEW:
pub async fn run_enrich_pipeline(
    pool: &SqlitePool,
    vectors: &dyn VectorStore,
    ...
```

**Step 2a: Replace vector_top_k call** (line 79):

```rust
// OLD:
let vector_hits = vector_top_k(vectors, &query_embedding, config.vector_top_k);

// NEW:
let vector_hits = match vectors.query(query_embedding.clone(), config.vector_top_k).await {
    Ok(hits) => hits,
    Err(e) => {
        warn!(scope_key, error = %e, "enrich: vector search failed");
        vec![]
    }
};
```

**Step 5: Replace cosine_dedup call** (line 111):

```rust
// OLD:
let fused = cosine_dedup(fused, vectors, config.dedup_threshold);

// NEW:
let dedup_ids: Vec<String> = fused.iter().map(|r| r.entity_id.clone()).collect();
let embeddings = vectors.get_vectors(&dedup_ids).await.unwrap_or_default();
let fused = cosine_dedup(fused, &embeddings, config.dedup_threshold);
```

**Delete `vector_top_k` function** (lines 262-288) — fully replaced by `VectorStore::query`.

**Import changes** (lines 1-24):
- Remove: `use std::hash::BuildHasher;`
- Add: `use crate::memory::vector::VectorStore;`
- Keep `use crate::memory::vector::VectorHit;` (still used by backend results)

**Test changes:**
- Delete `vector_top_k_returns_top_results` test (lines 551-564) — the function no longer exists. Coverage is provided by `VectorStore::query` tests in `vector/mod.rs`.

### Task 2.3: Refactor `run_store_pipeline` (store/mod.rs)

**File:** `runtime/core-rust/src/memory/store/mod.rs`

**Signature change** (line 55-64):

```rust
// OLD:
pub async fn run_store_pipeline(
    pool: &SqlitePool,
    vectors: &mut InMemoryVectorStore,
    graph: &mut PetGraphStore,
    ...

// NEW:
pub async fn run_store_pipeline(
    pool: &SqlitePool,
    vectors: &dyn VectorStore,
    graph: &mut PetGraphStore,
    ...
```

**Line 91: Replace `vectors.as_map()`:**

```rust
// OLD:
let outcome = resolve_conflict(pool, vectors.as_map(), graph, config, ...);

// NEW:
let outcome = resolve_conflict(pool, vectors, graph, config, ...);
```

**Line 110: `vectors.upsert(...)` stays the same** — already uses the trait. Just remove `&mut` from the parameter type.

**Import changes** (line 14):
```rust
// OLD:
use crate::memory::vector::{InMemoryVectorStore, VectorStore};

// NEW:
use crate::memory::vector::VectorStore;
```

### Task 2.4: No changes to assembler.rs

`cosine_dedup` at `retrieval/assembler.rs:44-69` keeps its `&HashMap<String, Vec<f32>, S>` signature. The caller (enrich pipeline, Task 2.2) builds the HashMap via `get_vectors()`.

### Verify

```bash
cd runtime/core-rust && cargo check
```

---

## Phase 3: MemoryEngine Integration

**File:** `runtime/core-rust/src/memory/mod.rs`

### Task 3.1: Change vectors field type

**Line 67:**
```rust
// OLD:
vectors: Arc<RwLock<InMemoryVectorStore>>,

// NEW:
vectors: Arc<dyn VectorStore>,
```

### Task 3.2: Accept vectors as constructor parameter

**Lines 97-117:**
```rust
// OLD:
pub async fn new(
    pool: SqlitePool,
    gateway_url: impl Into<String>,
    config: MemoryConfig,
) -> Result<Self, sqlx::Error> {
    ...
    vectors: Arc::new(RwLock::new(InMemoryVectorStore::new(config.vector_dimensions))),
    ...
}

// NEW:
pub async fn new(
    pool: SqlitePool,
    gateway_url: impl Into<String>,
    config: MemoryConfig,
    vectors: Arc<dyn VectorStore>,
) -> Result<Self, sqlx::Error> {
    ...
    Ok(Self {
        db: pool,
        vectors,
        graph: Arc::new(RwLock::new(graph)),
        extractor,
        embedder,
        config,
    })
}
```

### Task 3.3: Remove vector locks from enrich

**Lines 171-176:**
```rust
// OLD:
let vectors_guard = self.vectors.read().await;
let graph_guard = self.graph.read().await;
run_enrich_pipeline(&self.db, vectors_guard.as_map(), &graph_guard, ...)

// NEW:
let graph_guard = self.graph.read().await;
run_enrich_pipeline(&self.db, &*self.vectors, &graph_guard, ...)
```

### Task 3.4: Remove vector locks from store

**Lines 138-143 (`store_after_response`):**
```rust
// OLD:
let mut vectors_guard = self.vectors.write().await;
let mut graph_guard = self.graph.write().await;
run_store_pipeline(&self.db, &mut vectors_guard, &mut graph_guard, ...)

// NEW:
let mut graph_guard = self.graph.write().await;
run_store_pipeline(&self.db, &*self.vectors, &mut graph_guard, ...)
```

**Lines 200-205 (`MemoryClient::store` impl):**
Same change — remove vectors write lock, pass `&*self.vectors`.

### Task 3.5: Update imports

**Line 45:**
```rust
// OLD:
use crate::memory::vector::InMemoryVectorStore;

// NEW:
use crate::memory::vector::VectorStore;
```

Remove `use tokio::sync::RwLock;` only if graph is the sole remaining user — check first. Graph still uses `Arc<RwLock<PetGraphStore>>`, so keep the import.

### Task 3.6: Update doc comments

- Line 65-66: update to say "Vector store for semantic search" (remove "Write-locked during store operations" language).

### Verify

```bash
cd runtime/core-rust && cargo check
```

This will fail at bootstrap (`init_memory_client`) because `MemoryEngine::new` now requires a `vectors` param. Fix in Phase 5.

---

## Phase 4: ZvecVectorStore Implementation

### Task 4.1: Add cargo dependency and feature flag

**File:** `runtime/core-rust/Cargo.toml`

Add to `[features]`:
```toml
[features]
zvec = ["dep:zvec-bindings"]
```

Add to `[dependencies]`:
```toml
zvec-bindings = { git = "https://github.com/seanmozeik/zvecrs", optional = true }
```

### Task 4.2: Create zvec module

**New file:** `runtime/core-rust/src/memory/vector/zvec.rs`

**Register in `vector/mod.rs`:**
```rust
#[cfg(feature = "zvec")]
pub mod zvec;
```

**Implementation:**

```rust
//! zvec-backed HNSW vector store with on-disk persistence.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;

use zvec_bindings::{
    create_and_open, open, Collection, CollectionSchema, Doc, IndexParams, MetricType,
    QuantizeType, VectorQuery, VectorSchema,
};

use super::{VectorError, VectorHit, VectorStore};

const FIELD_NAME: &str = "embedding";

/// HNSW-backed vector store using zvec.
///
/// All operations use interior mutability via zvec's C++ engine.
/// Supports on-disk persistence — call [`VectorStore::flush`] to sync.
pub struct ZvecVectorStore {
    collection: Collection,
    dimensions: usize,
}

impl ZvecVectorStore {
    /// Open an existing collection or create a new one with HNSW index.
    pub fn open_or_create(path: &Path, dimensions: usize) -> Result<Self, VectorError> {
        // Try opening existing collection first.
        if let Ok(collection) = open(path) {
            return Ok(Self { collection, dimensions });
        }

        let mut schema = CollectionSchema::new("embeddings");
        schema
            .add_field(VectorSchema::fp32(FIELD_NAME, dimensions as u32).into())
            .map_err(|e| VectorError::Internal(format!("schema: {e}")))?;

        let collection = create_and_open(path, schema)
            .map_err(|e| VectorError::Internal(format!("create: {e}")))?;

        collection
            .create_index(
                FIELD_NAME,
                IndexParams::hnsw(16, 200, MetricType::Cosine, QuantizeType::Undefined),
            )
            .map_err(|e| VectorError::Internal(format!("index: {e}")))?;

        Ok(Self { collection, dimensions })
    }
}
```

**VectorStore impl:**

```rust
#[async_trait]
impl VectorStore for ZvecVectorStore {
    async fn upsert(&self, id: String, vector: Vec<f32>) -> Result<(), VectorError> {
        if vector.len() != self.dimensions {
            return Err(VectorError::DimensionMismatch {
                expected: self.dimensions,
                got: vector.len(),
            });
        }
        let doc = Doc::id(&id)
            .with_vector(FIELD_NAME, &vector)
            .map_err(|e| VectorError::Internal(format!("doc: {e}")))?;
        self.collection
            .upsert(&[doc])
            .map_err(|e| VectorError::Internal(format!("upsert: {e}")))?;
        Ok(())
    }

    async fn query(&self, vector: Vec<f32>, top_k: usize) -> Result<Vec<VectorHit>, VectorError> {
        if vector.len() != self.dimensions {
            return Err(VectorError::DimensionMismatch {
                expected: self.dimensions,
                got: vector.len(),
            });
        }
        let q = VectorQuery::new(FIELD_NAME)
            .topk(top_k)
            .vector(&vector)
            .map_err(|e| VectorError::Internal(format!("query: {e}")))?;
        let results = self
            .collection
            .query(q)
            .map_err(|e| VectorError::Internal(format!("search: {e}")))?;

        let mut hits = Vec::with_capacity(results.len());
        for doc in results.iter() {
            hits.push(VectorHit {
                entity_id: doc.pk(),
                score: f64::from(doc.score()),
            });
        }
        Ok(hits)
    }

    async fn delete(&self, id: &str) -> Result<(), VectorError> {
        self.collection
            .delete(&[id])
            .map_err(|e| VectorError::Internal(format!("delete: {e}")))?;
        Ok(())
    }

    async fn flush(&self) -> Result<(), VectorError> {
        self.collection
            .flush()
            .map_err(|e| VectorError::Internal(format!("flush: {e}")))?;
        Ok(())
    }

    async fn get_vectors(&self, ids: &[String]) -> Result<HashMap<String, Vec<f32>>, VectorError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let doc_map = self
            .collection
            .fetch(&id_refs)
            .map_err(|e| VectorError::Internal(format!("fetch: {e}")))?;

        let mut result = HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some(doc) = doc_map.get(id) {
                if let Some(vec) = doc.get_vector(FIELD_NAME) {
                    result.insert(id.clone(), vec);
                }
            }
        }
        Ok(result)
    }
}
```

**Important:** zvec with `MetricType::Cosine` returns cosine similarity scores where higher = more similar. If testing shows scores are inverted (distance instead of similarity), adjust: `score: 1.0 - f64::from(doc.score())`.

### Task 4.3: Add config field for vector store path

**File:** `runtime/core-rust/src/memory/config.rs`

Add field to `MemoryConfig` (after line 51):
```rust
/// Directory for persistent vector store data (zvec collection files).
/// Defaults to `{memory_root}/.index/vectors/`.
pub vector_store_path: Option<PathBuf>,
```

Add to `Default` impl (after line 71):
```rust
vector_store_path: None,
```

Add to `from_env()` (after line 155):
```rust
if let Ok(v) = std::env::var("MEMORY_VECTOR_STORE_PATH") {
    cfg.vector_store_path = Some(PathBuf::from(v));
}
```

Add resolver method to `impl MemoryConfig` (after line 198):
```rust
/// Resolve the vector store path, defaulting to `{memory_root}/.index/vectors/`.
pub fn resolved_vector_store_path(&self) -> PathBuf {
    self.vector_store_path
        .clone()
        .unwrap_or_else(|| self.memory_root.join(".index").join("vectors"))
}
```

### Verify

```bash
cd runtime/core-rust && cargo check --features zvec
```

---

## Phase 5: Bootstrap Wiring

**File:** `runtime/core-rust/src/bootstrap/mod.rs`

### Task 5.1: Update init_memory_client

**Lines 196-210:**

```rust
async fn init_memory_client(
    storage: &StorageHandle,
) -> Result<(Arc<dyn MemoryClient>, Arc<MemoryEngine>), BootstrapError> {
    let gateway_url = std::env::var("INFERENCE_GATEWAY_URL").unwrap_or_default();
    let config = MemoryConfig::from_env();

    let vectors = build_vector_store(&config)?;

    info!(gateway_url, "memory_client: initializing MemoryEngine");
    let engine = MemoryEngine::new(storage.pool.clone(), gateway_url, config, vectors)
        .await
        .map_err(|e| {
            error!(error = %e, "memory_engine init failed");
            BootstrapError::SubsystemFailed {
                subsystem: "memory_engine",
                cause: format!("{e}"),
            }
        })?;
    let engine = Arc::new(engine);
    let client: Arc<dyn MemoryClient> = engine.clone();
    Ok((client, engine))
}
```

### Task 5.2: Add build_vector_store function

```rust
/// Construct the vector store. Uses zvec when the `zvec` feature is enabled,
/// otherwise uses the brute-force in-memory store.
fn build_vector_store(config: &MemoryConfig) -> Result<Arc<dyn VectorStore>, BootstrapError> {
    #[cfg(feature = "zvec")]
    {
        use crate::memory::vector::zvec::ZvecVectorStore;

        zvec_bindings::init().map_err(|e| BootstrapError::SubsystemFailed {
            subsystem: "zvec",
            cause: format!("zvec init: {e}"),
        })?;

        let path = config.resolved_vector_store_path();
        std::fs::create_dir_all(&path).map_err(|e| BootstrapError::SubsystemFailed {
            subsystem: "zvec",
            cause: format!("create dir {}: {e}", path.display()),
        })?;

        let store = ZvecVectorStore::open_or_create(&path, config.vector_dimensions)
            .map_err(|e| BootstrapError::SubsystemFailed {
                subsystem: "zvec",
                cause: format!("open: {e}"),
            })?;

        info!(path = %path.display(), dimensions = config.vector_dimensions, "zvec vector store opened");
        Ok(Arc::new(store))
    }

    #[cfg(not(feature = "zvec"))]
    {
        use crate::memory::vector::InMemoryVectorStore;

        info!(dimensions = config.vector_dimensions, "using in-memory vector store");
        Ok(Arc::new(InMemoryVectorStore::new(config.vector_dimensions)))
    }
}
```

### Task 5.3: Update bootstrap test

The `bootstrap_initializes_memory_engine` test (line 696) calls `MemoryEngine::new` without a vectors param. Update to pass an `InMemoryVectorStore`:

```rust
use crate::memory::vector::InMemoryVectorStore;

let vectors: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new(384));
let engine = MemoryEngine::new(storage.pool.clone(), gateway_url, config, vectors)
    .await
    .expect("bootstrap should succeed");
```

### Task 5.4: Update handler test if needed

**File:** `runtime/core-rust/src/api/rpc/handler.rs:1051-1112`

If this test constructs a `MemoryEngine` directly, update to pass vectors. If it only uses `BootstrapMemoryClient`, no change needed.

### Verify

```bash
cd runtime/core-rust && cargo check && cargo test
cd runtime/core-rust && cargo check --features zvec
cd runtime/core-rust && cargo clippy --all-targets
cd runtime/core-rust && cargo clippy --all-targets --features zvec
```

---

## Commit Strategy

1. **Phase 0** — separate repo (`~/dev/zvecrs`), push to `seanmozeik/zvecrs`
2. **Phases 1-3 + 5** — single atomic commit in ultraclaw: trait evolution + pipeline refactoring + MemoryEngine + bootstrap. Compiles and tests pass with `InMemoryVectorStore`. Message: `refactor(memory): evolve VectorStore trait for interior mutability`
3. **Phase 4** — second commit: zvec implementation + Cargo dep + feature flag. Message: `feat(memory): add zvec HNSW vector store behind feature flag`

---

## Verification Checklist

- [ ] `cargo check` — compiles without zvec feature
- [ ] `cargo test` — all memory tests pass with InMemoryVectorStore
- [ ] `cargo check --features zvec` — compiles with zvec
- [ ] `cargo test --features zvec` — passes
- [ ] `cargo clippy --all-targets` — clean
- [ ] `cargo clippy --all-targets --features zvec` — clean

---

## zvec API Quick Reference

For implementors — key types and patterns from `zvec-bindings`:

```rust
// Init (call once at startup)
zvec_bindings::init()?;

// Create collection
let mut schema = CollectionSchema::new("name");
schema.add_field(VectorSchema::fp32("embedding", 384).into())?;
let collection = create_and_open("./path", schema)?;

// Open existing
let collection = open("./path")?;

// HNSW index
collection.create_index("embedding",
    IndexParams::hnsw(16, 200, MetricType::Cosine, QuantizeType::Undefined))?;

// Upsert
let doc = Doc::id("entity_id").with_vector("embedding", &vec_f32)?;
collection.upsert(&[doc])?;

// Query
let q = VectorQuery::new("embedding").topk(10).vector(&query_vec)?;
let results = collection.query(q)?;
for doc in results.iter() {
    let id = doc.pk();       // String
    let score = doc.score();  // f32
    let vec = doc.get_vector("embedding"); // Option<Vec<f32>>
}

// Fetch by PK
let doc_map = collection.fetch(&["id1", "id2"])?;
if let Some(doc) = doc_map.get("id1") {
    let vec = doc.get_vector("embedding"); // Option<Vec<f32>>
}

// Persistence
collection.flush()?;

// Delete
collection.delete(&["id1"])?;
```

**Thread safety:** `Collection` implements `Send + Sync`. All methods take `&self`.

**Error type:** `zvec_bindings::Error` with variants: `NotFound`, `AlreadyExists`, `InvalidArgument`, `InternalError`, `DimensionMismatch`, etc.
