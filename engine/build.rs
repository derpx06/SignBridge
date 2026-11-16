fn main() {
    // Route sherpa-onnx's ONNX Runtime API lookup through src/ort_arena.rs (GNU ld only).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")
    {
        println!("cargo:rustc-link-arg=-Wl,--wrap=OrtGetApiBase");
    }
}
