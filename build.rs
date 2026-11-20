// Embed a Common Controls v6 manifest in the GUI so its buttons and list use
// the modern Windows theme. Only the MSVC linker supports this without extra
// tools; other targets get the classic look.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg-bin=sw1tch=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=sw1tch=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
