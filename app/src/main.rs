fn main() {
    let (major, minor, patch) = opendrop_engine::projectm_version();
    println!("projectM {major}.{minor}.{patch}");
}
