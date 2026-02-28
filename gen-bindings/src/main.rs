/// Generates bindings.rs from the zvec C wrapper header for the current platform.
/// Usage: gen-bindings <header-path> <output-path>
fn main() {
    let mut args = std::env::args().skip(1);
    let header = args.next().expect("usage: gen-bindings <header-path> <output-path>");
    let output = args.next().expect("usage: gen-bindings <header-path> <output-path>");

    bindgen::Builder::default()
        .header(&header)
        .generate_comments(true)
        .clang_arg("-I/usr/include")
        .clang_arg("-I/usr/local/include")
        .generate()
        .unwrap_or_else(|e| panic!("Failed to generate bindings from {header}: {e}"))
        .write_to_file(&output)
        .unwrap_or_else(|e| panic!("Failed to write bindings to {output}: {e}"));

    println!("Generated {output}");
}
