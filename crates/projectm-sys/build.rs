//! Build script for projectm-sys
//!
//! Generates FFI bindings to libprojectM using bindgen

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to look in standard library paths
    println!("cargo:rustc-link-search=native=/usr/lib");

    // Link to projectM-4 and OpenGL
    // Use dylib to link to the shared library
    println!("cargo:rustc-link-lib=dylib=projectM-4");
    println!("cargo:rustc-link-lib=dylib=OpenGL");

    // Get include paths from pkg-config (but don't let it emit link instructions)
    let include_paths = pkg_config::Config::new()
        .atleast_version("4.0")
        .cargo_metadata(false)  // Don't emit cargo link instructions, we handle that above
        .probe("projectM-4")
        .map(|lib| lib.include_paths)
        .unwrap_or_default();

    // Generate bindings
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("projectm_.*")
        .allowlist_type("projectm_.*")
        .allowlist_var("PROJECTM_.*")
        // Block some problematic types
        .blocklist_type("max_align_t");

    // Add include paths from pkg-config
    for path in &include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
