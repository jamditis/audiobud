#[cfg(feature = "generate-bindings")]
fn main() {
    let output = std::path::Path::new("../src/bindings.ts");
    handy_app_lib::generate_typescript_bindings(output);
    println!("generated {}", output.display());
}

#[cfg(not(feature = "generate-bindings"))]
fn main() {
    eprintln!("run with --features generate-bindings");
    std::process::exit(2);
}
