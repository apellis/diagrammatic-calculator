// Caps the WebAssembly memory: without a maximum, one runaway computation in a browser worker can
// grow to the 4 GiB a 32-bit memory allows, and the page runs several workers. Past the cap an
// allocation fails, the instance traps, and the page replaces the worker.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("wasm32") {
        // 2 GiB, in bytes (a multiple of the 64 KiB page)
        println!("cargo:rustc-link-arg-cdylib=--max-memory=2147483648");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
