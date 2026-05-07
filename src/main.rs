//! # jhana-rs
//!
//! Voice-driven meditation guide for the Radxa Rock 5A.
//!
//! Phase 1: text prompt -> LLM streaming -> ratatui TUI display.
//! Runs on the Rock's tty1 (720x1280 portrait, `TerminusBold` 32x16 font).
//!
//! Logs to `jhana-rs.log` in the working directory for debugging.
//! Send SIGTERM or SIGINT to quit cleanly (no physical keyboard needed).
//! Hardware buttons: BACK=quit, ENTER=start, UP/DOWN=scroll.

mod gpio;
mod llm;
mod ui;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use log::{error, info};
use ratatui::{Terminal, backend::CrosstermBackend};
use simplelog::{Config, LevelFilter, WriteLogger};

use crate::gpio::ButtonEvent;
use crate::llm::LlmOutput;
use crate::ui::{App, AppState, render};

/// Default meditation type loaded on startup.
const DEFAULT_MEDITATION: &str = "test";

fn main() -> io::Result<()> {
    // File logger — all output goes to jhana-rs.log, not stdout/tty
    let log_file = std::fs::File::create("jhana-rs.log")?;
    WriteLogger::init(LevelFilter::Debug, Config::default(), log_file)
        .expect("failed to init logger");

    info!("jhana-rs starting");

    // GPIO button polling (returns None on non-Rock hardware)
    let button_rx = gpio::start_polling();
    if button_rx.is_some() {
        info!("GPIO buttons enabled");
    } else {
        info!("GPIO buttons not available (keyboard only)");
    }

    // LLM output channel — background streaming thread sends here
    let (llm_tx, llm_rx) = mpsc::channel::<LlmOutput>();

    // Signal handling — SIGTERM/SIGINT set this flag to quit the event loop
    let quit = Arc::new(AtomicBool::new(false));
    let quit_signal = Arc::clone(&quit);
    ctrlc_handler(&quit_signal);

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    info!("terminal initialized, entering event loop");

    let mut app = App::new();

    // Main event loop
    let result = run_loop(
        &mut terminal,
        &mut app,
        &quit,
        button_rx.as_ref(),
        &llm_tx,
        &llm_rx,
    );

    // Cleanup — always restore terminal state
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("jhana-rs exiting, restoring console");

    // Restore console: re-apply the large font and spawn a login prompt.
    // The TUI runs detached from tty1's session (via setsid), so there is
    // no shell to return to. We reset the console and start getty so the
    // user gets a login prompt with the correct font.
    restore_console();

    result
}

/// Event loop that checks for key presses, GPIO buttons, LLM output, and
/// the quit signal.
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    quit: &AtomicBool,
    button_rx: Option<&mpsc::Receiver<ButtonEvent>>,
    llm_tx: &mpsc::Sender<LlmOutput>,
    llm_rx: &mpsc::Receiver<LlmOutput>,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;

        // Check signal flag
        if quit.load(Ordering::Relaxed) {
            info!("quit signal received");
            break;
        }

        // Check GPIO buttons (non-blocking)
        if let Some(rx) = button_rx {
            while let Ok(event) = rx.try_recv() {
                info!("button: {event:?}");
                match event {
                    ButtonEvent::Back => return Ok(()),
                    ButtonEvent::Up => app.scroll_up(),
                    ButtonEvent::Down => app.scroll_down(),
                    ButtonEvent::Enter => handle_start(app, llm_tx),
                }
            }
        }

        // Drain LLM output channel (non-blocking)
        while let Ok(output) = llm_rx.try_recv() {
            match output {
                LlmOutput::Sentence(s) => {
                    info!("sentence: {s}");
                    app.token_count += estimate_tokens(&s);
                    app.push_sentence(s);
                }
                LlmOutput::Pause(n) => {
                    info!("pause: {n:.0}s");
                    app.push_sentence(format!("[pause {n:.0}s]"));
                }
                LlmOutput::Done => {
                    app.finish();
                    info!("generation complete, {} tokens", app.token_count);
                }
                LlmOutput::Error(e) => {
                    error!("LLM error: {e}");
                    app.push_sentence(format!("Error: {e}"));
                    app.finish();
                }
            }
        }

        // Poll keyboard with 100ms timeout
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    info!("quit key pressed");
                    break;
                }
                KeyCode::Up => app.scroll_up(),
                KeyCode::Down => app.scroll_down(),
                KeyCode::Enter => handle_start(app, llm_tx),
                other => {
                    info!("key: {other:?}");
                }
            }
        }
    }

    Ok(())
}

/// Handle START/ENTER action based on current state.
///
/// - Idle: load prompts and start LLM streaming
/// - Done: reset and return to idle for another session
/// - Generating/Paused: ignored (generation is already in progress)
fn handle_start(app: &mut App, llm_tx: &mpsc::Sender<LlmOutput>) {
    match app.state {
        AppState::Idle => {
            info!("starting meditation: {DEFAULT_MEDITATION}");
            match llm::load_prompts(DEFAULT_MEDITATION) {
                Ok((system, user)) => {
                    app.start_generating();
                    llm::start_streaming(llm_tx.clone(), system, user);
                }
                Err(e) => {
                    error!("Failed to load prompts: {e}");
                    app.push_sentence(format!("Error: {e}"));
                }
            }
        }
        AppState::Done => {
            info!("resetting to idle");
            app.reset();
        }
        AppState::Generating | AppState::Paused => {
            info!("start pressed during generation — ignored");
        }
    }
}

/// Estimate token count from text length (rough heuristic for TUI display).
///
/// Uses ~4 characters per token (GPT-style average). This is only for the
/// live speed display in the footer — exact counts aren't needed since the
/// actual SSE stream delivers one token per `data:` event.
fn estimate_tokens(text: &str) -> u32 {
    #[expect(clippy::cast_possible_truncation)]
    let estimate = (text.len() as u32 / 4).max(1);
    estimate
}

/// Register SIGTERM/SIGINT handler that sets the quit flag.
fn ctrlc_handler(quit: &Arc<AtomicBool>) {
    let quit = Arc::clone(quit);
    if let Err(e) = signal_hook::flag::register(signal_hook::consts::SIGTERM, quit.clone()) {
        error!("failed to register SIGTERM handler: {e}");
    }
    if let Err(e) = signal_hook::flag::register(signal_hook::consts::SIGINT, quit) {
        error!("failed to register SIGINT handler: {e}");
    }
}

/// Restore the console after TUI exit.
///
/// The TUI runs detached via `setsid` so there is no parent shell on tty1.
/// This function clears the screen, re-applies the large console font, and
/// restarts `getty` on tty1 to give the user a login prompt they can type into.
fn restore_console() {
    use std::process::Command;

    let commands: &[&[&str]] = &[
        // Reset terminal and clear screen
        &["bash", "-c", "echo -e '\\033c' > /dev/tty1"],
        // Re-apply large console font (may have been reset by alternate screen)
        &[
            "setfont",
            "/usr/share/consolefonts/Uni3-TerminusBold32x16.psf.gz",
        ],
        // Restart getty on tty1 for a login prompt
        &["systemctl", "restart", "getty@tty1.service"],
    ];

    for cmd in commands {
        match Command::new(cmd[0]).args(&cmd[1..]).status() {
            Ok(s) if s.success() => info!("restore: {} ok", cmd.join(" ")),
            Ok(s) => error!("restore: {} exited {s}", cmd.join(" ")),
            Err(e) => error!("restore: {} failed: {e}", cmd.join(" ")),
        }
    }
}
