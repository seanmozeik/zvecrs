use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=ZVEC_PREBUILT_DIR");

    let prebuilt_dir = PathBuf::from(
        env::var("ZVEC_PREBUILT_DIR")
            .expect("ZVEC_PREBUILT_DIR must be set — provide path to pre-built zvec artifacts"),
    );

    // Copy pre-generated bindings to OUT_DIR.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::copy(prebuilt_dir.join("bindings.rs"), out_dir.join("bindings.rs"))
        .expect("Failed to copy bindings.rs from ZVEC_PREBUILT_DIR");

    link_libraries(&prebuilt_dir);
}

fn link_libraries(prebuilt: &std::path::Path) {
    println!("cargo:rustc-link-search=native={}", prebuilt.display());

    println!("cargo:rustc-link-lib=static=zvec_c_wrapper");

    // zvec core libraries (whole-archive: required for static init and test linking)
    for lib in &["zvec_core", "zvec_ailego", "zvec_db"] {
        println!("cargo:rustc-link-lib=static:+whole-archive={}", lib);
    }

    // Third-party dependencies (whole-archive for test linking)
    for lib in &[
        "parquet",
        "arrow_acero",
        "arrow_dataset",
        "arrow_compute",
        "arrow",
        "arrow_bundled_dependencies",
        "roaring",
        "rocksdb",
        "lz4",
        "protobuf",
        "protoc",
        "boost_thread",
        "boost_atomic",
        "boost_chrono",
        "boost_container",
        "boost_date_time",
        "boost_locale",
        "boost_charconv",
        "glog",
        "gflags_nothreads",
        "antlr4-runtime",
    ] {
        println!("cargo:rustc-link-lib=static:+whole-archive={}", lib);
    }

    // System libraries
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=m");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
    }
}
