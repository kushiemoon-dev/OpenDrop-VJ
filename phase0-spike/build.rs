use std::env;
use std::path::PathBuf;

fn main() {
    // Arch's projectM-4.pc emits a broken `Libs: -l:projectM-4` (missing the `lib`
    // prefix the actual `libprojectM-4.so` needs), so don't trust its cargo_metadata
    // link directives; probe cflags/libdirs only and link manually below.
    let projectm = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("projectM-4")
        .expect("projectM-4.pc not found (pacman -S libprojectm)");

    for libdir in &projectm.link_paths {
        println!("cargo:rustc-link-search=native={}", libdir.display());
    }
    println!("cargo:rustc-link-lib=dylib=projectM-4");
    println!("cargo:rustc-link-lib=dylib=OpenGL");

    let mut builder = bindgen::Builder::default()
        .header_contents("wrapper.h", "#include <projectM-4/projectM.h>\n")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for inc in &projectm.include_paths {
        builder = builder.clang_arg(format!("-I{}", inc.display()));
    }

    let bindings = builder.generate().expect("bindgen failed on projectM headers");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("projectm_bindings.rs"))
        .expect("failed to write bindings");
}
