fn main() {
    println!(
        "cargo:rustc-env=LM_BUILD_TARGET={}",
        std::env::var("TARGET").expect("Cargo supplies TARGET to build scripts")
    );
}
