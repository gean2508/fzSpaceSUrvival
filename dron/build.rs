fn main() {
    cc::Build::new()
        .file("csrc/iafz_64.c")
        .file("csrc/iafz_64_abordo.c")
        .opt_level(3)
        .warnings(false)
        .compile("iafz64");
    println!("cargo:rerun-if-changed=csrc/iafz_64.c");
    println!("cargo:rerun-if-changed=csrc/iafz_64.h");
    println!("cargo:rerun-if-changed=csrc/iafz_64_abordo.c");
}
