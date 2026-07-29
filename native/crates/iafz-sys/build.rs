// Compila el kernel IAFZ-64 (C plano, solo libc — sin OpenMP ni deps) y lo linkea
// estático. `cc` cross-compila para Android automáticamente vía cargo-ndk (usa el
// clang del NDK), así que el mismo cerebro corre en el Redmi.
fn main() {
    println!("cargo:rerun-if-changed=csrc/iafz_64.c");
    println!("cargo:rerun-if-changed=csrc/iafz_64.h");
    cc::Build::new()
        .file("csrc/iafz_64.c")
        .warnings(false)
        .compile("iafz");
}
