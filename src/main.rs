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
mod ui;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::ui::{App, render};

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
    app.lines = vec![
        String::from("Close your eyes and take a deep breath in."),
        String::new(),
        String::from("[pause 5s]"),
        String::new(),
        String::from("Now slowly exhale, releasing any tension you feel."),
        String::new(),
        String::from("[pause 3s]"),
        String::new(),
        String::from("Let your shoulders drop."),
    ];
    app.state = String::from("Demo");

    // Main event loop
    let result = run_loop(&mut terminal, &mut app, &quit, button_rx.as_ref());

    // Cleanup — always restore terminal state
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("jhana-rs exiting");

    result
}

/// Event loop that checks for key presses, GPIO buttons, and the quit signal.
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    quit: &AtomicBool,
    button_rx: Option<&std::sync::mpsc::Receiver<ButtonEvent>>,
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
                    ButtonEvent::Up => {
                        app.scroll_up();
                    }
                    ButtonEvent::Down => {
                        app.scroll_down();
                    }
                    ButtonEvent::Enter => {
                        info!("enter pressed (start)");
                    }
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
                KeyCode::Up => {
                    app.scroll_up();
                    info!("scroll up (key)");
                }
                KeyCode::Down => {
                    app.scroll_down();
                    info!("scroll down (key)");
                }
                KeyCode::Enter => {
                    info!("enter key pressed");
                }
                other => {
                    info!("key: {other:?}");
                }
            }
        }
    }

    Ok(())
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
