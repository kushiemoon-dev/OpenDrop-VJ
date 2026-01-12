//! Raw FFI bindings to libprojectM
//!
//! This crate provides low-level, unsafe bindings to the libprojectM C API.
//! For a safe, idiomatic Rust API, use the `projectm-rs` crate instead.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn bindings_exist() {
        // Basic test to ensure bindings are generated
        // Actual functionality tests will be in projectm-rs
    }
}
