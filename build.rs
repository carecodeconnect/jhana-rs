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
        eprintln!("slint-build: {e}");
        // Fail the build with a clear message; the previous silent-swallow
        // produced confusing "MainWindow / LogEntry not found" errors at
        // use site instead of pointing at the actual .slint parse fault.
        std::process::exit(1);
    }
}
