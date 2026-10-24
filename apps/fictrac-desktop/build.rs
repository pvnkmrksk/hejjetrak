fn main() {
    println!("cargo:rerun-if-changed=Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && let Ok(root) = std::env::var("CARGO_MANIFEST_DIR")
    {
        println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{root}/Info.plist");
    }
}
