// build.rs — compile the Slint .slint UI files into Rust source so
// the `jhana-rs-slint` binary can include them via `slint::include_modules!()`.
//
// Only the slint binary uses these. The main `jhana-rs` (ratatui) and
// `jhana-llm-server` builds incur the slint-build dependency but the
// generated code is harmless if unused.
//
// See `docs/18_SLINT.md` for the Slint-side architecture.

fn main() {
    println!("cargo:rerun-if-changed=ui/jhana.slint");
    if let Err(e) = slint_build::compile("ui/jhana.slint") {
        // Print the error but don't fail the build — non-slint binaries
        // can still compile. The slint binary will error out at use site
        // if compilation actually failed.
        eprintln!("slint-build: {e}");
    }
}
