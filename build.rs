fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/painty.ico");

    // A build script runs on the machine that compiles, so `cfg!(target_os)` would
    // describe that machine. Cargo gives the real target in this variable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/painty.ico");
        res.compile().expect("impossible d'intégrer l'icône");
    }
}
