fn main() {
    // migrations are embedded into the final binary at compile time, so it's
    // necessary to rebuild when they change even if the Rust source hasn't
    println!("cargo:rerun-if-changed=migrations")
}
