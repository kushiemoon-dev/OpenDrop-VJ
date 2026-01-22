//! Build script for projectm-sys
//!
//! Generates FFI bindings to libprojectM using bindgen

use std::env;
use std::path::PathBuf;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Check if we have vcpkg-provided projectM (check BEFORE pkg-config)
    let use_vcpkg = env::var("PROJECTM_4_DIR").is_ok() || env::var("PROJECTM_4_LIB_DIR").is_ok();

    // Try pkg-config only if not using vcpkg
    let pkg_config_result = if use_vcpkg {
        None
    } else {
        let pkg_names = ["libprojectM", "projectM-4", "libprojectM-4", "projectm"];
        pkg_names.iter().find_map(|name| {
            pkg_config::Config::new()
                .atleast_version("4.0")
                .probe(name)
                .ok()
        })
    };

    let include_paths = if let Some(lib) = &pkg_config_result {
        // pkg-config found projectM, it will emit the link instructions
        lib.include_paths.clone()
    } else {
        // Fallback: manual configuration
        let mut include_paths = Vec::new();

        // Check for VCPKG_ROOT or PROJECTM_4_DIR (works on all platforms)
        let mut found_vcpkg = false;
        if let Ok(projectm_dir) = env::var("PROJECTM_4_DIR") {
            println!("cargo:rustc-link-search=native={}/lib", projectm_dir);
            include_paths.push(PathBuf::from(format!("{}/include", projectm_dir)));
            found_vcpkg = true;
        }
        if let Ok(projectm_lib_dir) = env::var("PROJECTM_4_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", projectm_lib_dir);
            found_vcpkg = true;
        }

        if target_os == "windows" {
            println!("cargo:rustc-link-lib=dylib=projectM-4");
        } else {
            // Linux/macOS
            if found_vcpkg {
                // vcpkg provides static libraries
                println!("cargo:rustc-link-lib=static=projectM-4");
                println!("cargo:rustc-link-lib=static=projectM_eval");
                println!("cargo:rustc-link-lib=static=glm");
                // System OpenGL
                println!("cargo:rustc-link-lib=dylib=GL");
                println!("cargo:rustc-link-lib=dylib=stdc++");
            } else {
                // Standard system paths as fallback (dynamic linking)
                println!("cargo:rustc-link-search=native=/usr/lib");
                println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
                println!("cargo:rustc-link-search=native=/usr/local/lib");
                include_paths.push(PathBuf::from("/usr/include"));
                include_paths.push(PathBuf::from("/usr/include/projectM-4"));
                include_paths.push(PathBuf::from("/usr/local/include"));
                println!("cargo:rustc-link-lib=dylib=projectM-4");
                println!("cargo:rustc-link-lib=dylib=GL");
            }
        }

        include_paths
    };

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
