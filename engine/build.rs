use std::env;
use std::path::PathBuf;

fn main() {
    let include_paths = probe_projectm();

    let mut builder = bindgen::Builder::default()
        .header_contents("wrapper.h", "#include <projectM-4/projectM.h>\n")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for inc in &include_paths {
        builder = builder.clang_arg(format!("-I{}", inc.display()));
    }

    let bindings = builder.generate().expect("bindgen failed on projectM headers");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("projectm_bindings.rs"))
        .expect("failed to write bindings");
}

#[cfg(not(target_os = "windows"))]
fn probe_projectm() -> Vec<PathBuf> {
    // Arch's projectM-4.pc emits a broken `Libs: -l:projectM-4` (missing the `lib`
    // prefix the actual `libprojectM-4.so` needs), so don't trust its cargo_metadata
    // link directives: probe cflags/libdirs only and link manually below.
    let projectm = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("projectM-4")
        .expect("projectM-4.pc not found (pacman -S libprojectm)");

    for libdir in &projectm.link_paths {
        println!("cargo:rustc-link-search=native={}", libdir.display());
    }
    println!("cargo:rustc-link-lib=dylib=projectM-4");
    println!("cargo:rustc-link-lib=dylib=OpenGL");

    projectm.include_paths
}

#[cfg(target_os = "windows")]
fn probe_projectm() -> Vec<PathBuf> {
    // vcpkg's projectm port (vcpkg install projectm:x64-windows) emits correct
    // cargo_metadata link directives itself, so we only need its include dirs
    // to feed the same bindgen pipeline the non-Windows branch above uses.
    //
    // Pin the triplet explicitly: vcpkg-rs defaults to "x64-windows-static-md"
    // for MSVC targets, but we installed the dynamic "x64-windows" triplet.
    // vcpkg-rs requires an explicit opt-in for dynamic triplets (the resulting
    // DLLs must be discoverable at runtime), so set that here rather than
    // requiring every dev/CI machine to export it themselves.
    env::set_var("VCPKGRS_DYNAMIC", "1");

    let projectm = vcpkg::Config::new()
        .target_triplet("x64-windows")
        .find_package("projectm")
        .expect("projectm not found via vcpkg (vcpkg install projectm:x64-windows)");

    projectm.include_paths
}
