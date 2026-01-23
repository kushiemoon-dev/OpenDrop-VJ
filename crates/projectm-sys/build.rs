//! Build script for projectm-sys
//!
//! Generates FFI bindings to libprojectM using bindgen

use std::env;
use std::fs;
use std::path::PathBuf;

/// Result of finding projectM library
struct LibraryInfo {
    name: String,
    is_static: bool,
}

/// Find projectM library files in the given directory
/// Returns the base name and whether it's static or dynamic
fn find_projectm_lib(lib_dir: &PathBuf, target_os: &str) -> Option<LibraryInfo> {
    // Possible library name patterns to search for
    // vcpkg uses "projectM4" (no hyphen) as the CMake package name
    let patterns = [
        "projectM4",      // vcpkg naming (most likely on Windows)
        "projectm4",      // lowercase variant
        "projectM-4",     // hyphenated variant
        "projectm-4",     // lowercase hyphenated
    ];

    // Debug: list directory contents
    if let Ok(entries) = fs::read_dir(lib_dir) {
        let files: Vec<_> = entries
            .flatten()
            .filter_map(|e| e.path().file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|name| {
                name.ends_with(".lib") || name.ends_with(".a") ||
                name.ends_with(".so") || name.contains(".so.")
            })
            .collect();
        eprintln!("cargo:warning=Library directory {} contains: {:?}", lib_dir.display(), files);
    }

    if let Ok(entries) = fs::read_dir(lib_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                let file_lower = file_name.to_lowercase();

                // Check for static libraries first (.lib on Windows, .a on Unix)
                for pattern in &patterns {
                    let pattern_lower = pattern.to_lowercase();

                    // Windows static: projectM4.lib
                    if target_os == "windows" && file_lower == format!("{}.lib", pattern_lower) {
                        return Some(LibraryInfo {
                            name: file_name.trim_end_matches(".lib").to_string(),
                            is_static: true,
                        });
                    }

                    // Unix static: libprojectM-4.a
                    if target_os != "windows" && file_lower == format!("lib{}.a", pattern_lower) {
                        return Some(LibraryInfo {
                            name: pattern.to_string(),
                            is_static: true,
                        });
                    }
                }

                // Check for dynamic libraries (.so on Unix)
                if target_os != "windows" {
                    for pattern in &patterns {
                        let pattern_lower = pattern.to_lowercase();
                        // libprojectM-4.so or libprojectM-4.so.4.x.x
                        if file_lower.starts_with(&format!("lib{}.", pattern_lower)) &&
                           (file_lower.contains(".so") || file_lower.ends_with(".so")) {
                            return Some(LibraryInfo {
                                name: pattern.to_string(),
                                is_static: false,
                            });
                        }
                    }
                }
            }
        }
    }

    // Fallback: scan for any projectm library (exclude playlist and eval - they're auxiliary)
    if let Ok(entries) = fs::read_dir(lib_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                let file_lower = file_name.to_lowercase();
                // Skip auxiliary libraries - we only want the core library
                if file_lower.contains("playlist") || file_lower.contains("eval") {
                    continue;
                }
                if file_lower.contains("projectm") {
                    if file_name.ends_with(".lib") {
                        let name = file_name.trim_end_matches(".lib").to_string();
                        eprintln!("cargo:warning=Found projectM library: {} (static)", file_name);
                        return Some(LibraryInfo { name, is_static: true });
                    } else if file_name.ends_with(".a") {
                        let name = file_name.trim_end_matches(".a").trim_start_matches("lib").to_string();
                        eprintln!("cargo:warning=Found projectM library: {} (static)", file_name);
                        return Some(LibraryInfo { name, is_static: true });
                    } else if file_name.contains(".so") {
                        // Extract base name: libprojectM-4.so.4.1.0 -> projectM-4
                        let name = file_name
                            .split(".so")
                            .next()
                            .unwrap_or(file_name)
                            .trim_start_matches("lib")
                            .to_string();
                        eprintln!("cargo:warning=Found projectM library: {} (dynamic)", file_name);
                        return Some(LibraryInfo { name, is_static: false });
                    }
                }
            }
        }
    }
    None
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Check if we have vcpkg-provided projectM (check BEFORE pkg-config)
    let use_vcpkg = env::var("PROJECTM_4_DIR").is_ok() || env::var("PROJECTM_4_LIB_DIR").is_ok();

    // Try pkg-config only if not using vcpkg
    let pkg_config_result = if use_vcpkg {
        None
    } else {
        // Try projectM-4 first (version 4.x), then fallback to older names
        let pkg_names = ["projectM-4", "libprojectM-4", "libprojectM", "projectm"];
        pkg_names.iter().find_map(|name| {
            pkg_config::Config::new()
                .atleast_version("4.0")
                .probe(name)
                .ok()
        })
    };

    let include_paths = if let Some(lib) = &pkg_config_result {
        // pkg-config found projectM - but we need to emit link instructions explicitly
        // because pkg-config returns "-l:projectM-4" which Cargo doesn't handle well
        if target_os != "windows" {
            println!("cargo:rustc-link-lib=dylib=projectM-4");
        }
        lib.include_paths.clone()
    } else {
        // Fallback: manual configuration
        let mut include_paths = Vec::new();

        // Check for VCPKG_ROOT or PROJECTM_4_DIR (works on all platforms)
        let mut found_vcpkg = false;
        if let Ok(projectm_dir) = env::var("PROJECTM_4_DIR") {
            let base_path = PathBuf::from(&projectm_dir);
            let lib_path = base_path.join("lib");
            let include_path = base_path.join("include");
            println!("cargo:rustc-link-search=native={}", lib_path.display());
            // Also check manual-link directory (vcpkg puts some C++ libs here)
            let manual_link_path = lib_path.join("manual-link");
            if manual_link_path.exists() {
                println!("cargo:rustc-link-search=native={}", manual_link_path.display());
            }
            eprintln!("cargo:warning=Adding include path: {}", include_path.display());
            include_paths.push(include_path);
            found_vcpkg = true;
        }
        // Explicit include directory override
        if let Ok(include_dir) = env::var("PROJECTM_4_INCLUDE_DIR") {
            let include_path = PathBuf::from(&include_dir);
            eprintln!("cargo:warning=Adding explicit include path: {}", include_path.display());
            include_paths.push(include_path);
        }
        if let Ok(projectm_lib_dir) = env::var("PROJECTM_4_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", projectm_lib_dir);
            // Also check manual-link subdirectory
            let manual_link_path = PathBuf::from(&projectm_lib_dir).join("manual-link");
            if manual_link_path.exists() {
                println!("cargo:rustc-link-search=native={}", manual_link_path.display());
            }
            found_vcpkg = true;
        }

        if found_vcpkg {
            // Determine the library directory to search
            let lib_dir = if let Ok(lib_dir) = env::var("PROJECTM_4_LIB_DIR") {
                PathBuf::from(lib_dir)
            } else if let Ok(projectm_dir) = env::var("PROJECTM_4_DIR") {
                PathBuf::from(projectm_dir).join("lib")
            } else {
                PathBuf::new()
            };

            // Try to find the actual library name and type
            let lib_info = find_projectm_lib(&lib_dir, &target_os);
            let (projectm_lib_name, use_static) = match &lib_info {
                Some(info) => (info.name.clone(), info.is_static),
                None => ("projectM-4".to_string(), target_os == "windows"),
            };

            // Derive playlist library name from main library name
            // If main lib is "projectM4", playlist is "projectM4-playlist"
            // If main lib is "projectM-4", playlist is "projectM-4-playlist"
            let playlist_lib_name = format!("{}-playlist", projectm_lib_name);

            eprintln!("cargo:warning=Using projectM library: {} (static={})", projectm_lib_name, use_static);
            eprintln!("cargo:warning=Using playlist library: {}", playlist_lib_name);

            if target_os == "windows" {
                // Windows always uses static linking with vcpkg
                // vcpkg projectM includes: core lib, eval, and playlist
                println!("cargo:rustc-link-lib=static={}", projectm_lib_name);
                println!("cargo:rustc-link-lib=static={}", playlist_lib_name);
                println!("cargo:rustc-link-lib=static=projectM_eval");
                // GLEW for OpenGL extension loading (projectM dependency)
                // Try multiple names: glew32s (static), glew32, libglew32
                // vcpkg static build typically uses glew32 or libglew32
                let glew_lib = if lib_dir.join("glew32s.lib").exists() {
                    "glew32s"
                } else if lib_dir.join("libglew32.lib").exists() {
                    "libglew32"
                } else {
                    "glew32"
                };
                eprintln!("cargo:warning=Using GLEW library: {}", glew_lib);
                println!("cargo:rustc-link-lib=static={}", glew_lib);
                // Windows OpenGL and system libraries
                println!("cargo:rustc-link-lib=opengl32");
                println!("cargo:rustc-link-lib=gdi32");
                println!("cargo:rustc-link-lib=user32");
            } else if use_static {
                // Linux/macOS with static vcpkg libs
                println!("cargo:rustc-link-lib=static={}", projectm_lib_name);
                println!("cargo:rustc-link-lib=static={}", playlist_lib_name);
                println!("cargo:rustc-link-lib=static=projectM_eval");
                println!("cargo:rustc-link-lib=dylib=GL");
                println!("cargo:rustc-link-lib=dylib=stdc++");
            } else {
                // Linux/macOS with dynamic vcpkg libs
                println!("cargo:rustc-link-lib=dylib={}", projectm_lib_name);
                println!("cargo:rustc-link-lib=dylib=GL");
                println!("cargo:rustc-link-lib=dylib=stdc++");
            }
        } else if target_os != "windows" {
            // Standard system paths as fallback (dynamic linking) - Linux only
            println!("cargo:rustc-link-search=native=/usr/lib");
            println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
            println!("cargo:rustc-link-search=native=/usr/local/lib");
            include_paths.push(PathBuf::from("/usr/include"));
            include_paths.push(PathBuf::from("/usr/include/projectM-4"));
            include_paths.push(PathBuf::from("/usr/local/include"));
            println!("cargo:rustc-link-lib=dylib=projectM-4");
            println!("cargo:rustc-link-lib=dylib=GL");
        } else {
            // Windows without vcpkg - error out
            panic!("Windows build requires vcpkg. Set PROJECTM_4_DIR or PROJECTM_4_LIB_DIR environment variables.");
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
