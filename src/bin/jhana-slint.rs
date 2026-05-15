//! `jhana-rs-slint` — Phase C variant of the meditation app.
//!
//! Same agent_loop / LLM / STT / TTS modules as the main `jhana-rs`
//! binary, but the UI layer is **Slint** rendering directly to DRM/KMS
//! via the `linuxkms` backend. No kmscon, no compositor, no Linux VT.
//!
//! Aesthetic: Tron / WarGames / Weird Science (see
//! `docs/17_DISPLAY.md` § Visual direction and `docs/18_SLINT.md`).
//!
//! C-1 milestone: scaffold. Opens the window, renders the static
//! layout from `ui/jhana.slint`. AgentEvent bridge is C-3.

// Pull in the generated Slint code. `slint-build` (called from
// `build.rs`) compiles `ui/jhana.slint` into Rust at build time.
slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialise the file logger so we get the same observability the
    // main binary has. Logs to `jhana-rs.log` in the working dir.
    let log_file = std::fs::File::create("jhana-rs.log")?;
    simplelog::WriteLogger::init(
        log::LevelFilter::Info,
        simplelog::Config::default(),
        log_file,
    )
    .ok();
    log::info!("jhana-rs-slint starting");

    // The Slint backend is selected by the SLINT_BACKEND env var at
    // runtime — the systemd unit sets `SLINT_BACKEND=linuxkms` so
    // Slint renders directly to DRM/KMS without needing a compositor.
    // For local development on a workstation, set
    // `SLINT_BACKEND=winit` (or unset and let Slint pick).
    let main_window = MainWindow::new()?;

    // C-1: hardcoded sample state so we can see all three regions.
    // C-3 replaces this block with an AgentEvent receiver fed by the
    // existing agent_loop thread.
    main_window.set_state("idle".into());
    main_window.set_active_tool("".into());
    main_window.set_current_say("be still".into());
    main_window.set_pause_remaining(0);

    let sample_log = slint::VecModel::from(vec![
        LogEntry {
            kind: "system".into(),
            text: "ready".into(),
        },
        LogEntry {
            kind: "user".into(),
            text: "loving-kindness please".into(),
        },
        LogEntry {
            kind: "agent".into(),
            text: "let's begin with a breath".into(),
        },
        LogEntry {
            kind: "tool".into(),
            text: "ring_bell()".into(),
        },
    ]);
    main_window.set_log_entries(slint::ModelRc::new(sample_log));

    log::info!("Slint main window built; entering event loop");
    main_window.run()?;
    log::info!("jhana-rs-slint exiting");
    Ok(())
}
