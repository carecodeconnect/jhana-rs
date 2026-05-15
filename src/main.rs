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

mod config;
mod gpio;
mod llm;
mod stt;
mod tts;
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
use crate::stt::SttResult;
use crate::ui::{App, AppState, render};

// Welcome lines now come from config/jhana.json → ui.welcome_lines so
// you can edit the greeting without recompiling. See src/config.rs.

// Default meditation type now lives in config/jhana.json
// (ui.default_meditation). Edit + restart to switch the few-shot
// example shown to the LLM — no recompile.

fn main() -> io::Result<()> {
    // File logger — all output goes to jhana-rs.log, not stdout/tty
    let log_file = std::fs::File::create("jhana-rs.log")?;
    WriteLogger::init(LevelFilter::Info, Config::default(), log_file)
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

    // TTS background thread — receives sentences to speak aloud.
    // Spawned before STT so we can hand a clone of tts_tx to the STT
    // thread; the "Speak now" cue now routes through the TTS thread
    // (instead of paplay-direct) to keep it from overlapping any
    // in-flight TTS output.
    let tts_tx = tts::start();

    // STT background thread — receives listen commands, sends back transcriptions
    let (stt_result_tx, stt_result_rx) = mpsc::channel::<SttResult>();
    let stt_tx = stt::start(stt_result_tx, tts_tx.clone());

    // Pre-load the RKLLM model in the background so first ENTER press
    // doesn't pay the 37–74 s NPU init. Welcome speech + STT cold load
    // run in parallel with this; by the time the user actually speaks
    // and SenseVoice transcribes, the LLM should be ready.
    llm::preload();

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
    app.push_sentence("Loading models...".to_string());

    // Two-stage spoken status so the user gets an audible signal
    // long before the slow LLM cold-load (~2 min) finishes.
    //
    //   1. As soon as SenseVoice finishes (~22 s), say "Loading
    //      models, please wait" so the user knows the device is
    //      alive and hasn't crashed.
    //   2. Once both STT and LLM are ready (~2 min total cold),
    //      say the full welcome ("Welcome to jhana-rs. Press the
    //      enter button..."). The user can press ENTER from then on.
    {
        use std::sync::atomic::Ordering;
        let tts_tx_for_welcome = tts_tx.clone();
        std::thread::Builder::new()
            .name("welcome".into())
            .spawn(move || {
                // Stage 1: wait for STT only.
                while !stt::STT_READY.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                info!("STT ready — 'loading' announcement");
                let _ = tts_tx_for_welcome.send(tts::TtsCommand::Speak(
                    "Loading the meditation model. Please wait.".to_string(),
                ));

                // Stage 2: wait for LLM, then welcome.
                // No app-side bell here on purpose — the bell is a
                // model-emitted tool call ([BELL] in the meditation
                // stream), not a startup flourish.
                while !llm::LLM_READY.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                info!("LLM ready — welcome");
                for line in &config::get().ui.welcome_lines {
                    let _ = tts_tx_for_welcome.send(tts::TtsCommand::Speak(line.clone()));
                }
            })
            .expect("failed to spawn welcome thread");
    }

    // Main event loop
    let result = run_loop(
        &mut terminal,
        &mut app,
        &quit,
        button_rx.as_ref(),
        &llm_tx,
        &llm_rx,
        &tts_tx,
        &stt_tx,
        &stt_result_rx,
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
    tts_tx: &mpsc::Sender<tts::TtsCommand>,
    stt_tx: &mpsc::Sender<stt::SttCommand>,
    stt_rx: &mpsc::Receiver<SttResult>,
) -> io::Result<()> {
    loop {
        // Transition out of the Loading screen once both models report ready.
        // Cheap atomic loads each tick; the App method is a no-op after the
        // first call.
        if stt::STT_READY.load(Ordering::Acquire) && llm::LLM_READY.load(Ordering::Acquire) {
            app.finish_loading();
        }

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
                    ButtonEvent::Enter => handle_start(app, stt_tx),
                }
            }
        }

        // Drain STT results (non-blocking)
        while let Ok(result) = stt_rx.try_recv() {
            match result {
                SttResult::Recording => {
                    info!("STT: recording from mic");
                    app.push_console("Listening...".to_string());
                }
                SttResult::Processing => {
                    info!("STT: processing audio");
                    app.push_console("Transcribing...".to_string());
                }
                SttResult::Transcribed(text) => {
                    info!("STT transcribed: {text}");
                    app.reset();
                    app.push_console(format!("You said: {text}"));
                    // Feed transcription to LLM as the user prompt
                    match llm::load_prompts(&config::get().ui.default_meditation) {
                        Ok((system, _user)) => {
                            app.start_generating();
                            // Use the transcribed text as the user prompt
                            llm::start_streaming(llm_tx.clone(), system, text);
                        }
                        Err(e) => {
                            error!("Failed to load prompts: {e}");
                            app.push_console(format!("Error: {e}"));
                        }
                    }
                }
                SttResult::Error(e) => {
                    error!("STT error: {e}");
                    app.push_console(format!("STT Error: {e}"));
                    app.finish();
                }
            }
        }

        // Drain LLM output channel (non-blocking)
        while let Ok(output) = llm_rx.try_recv() {
            match output {
                LlmOutput::Sentence(s) => {
                    info!("sentence: {s}");
                    app.token_count += estimate_tokens(&s);
                    // Send to TTS thread for spoken output
                    let _ = tts_tx.send(tts::TtsCommand::Speak(s.clone()));
                    app.push_sentence(s);
                }
                LlmOutput::Pause(n) => {
                    info!("pause: {n:.0}s");
                    app.push_sentence(format!("[pause {n:.0}s]"));
                    // Forward to TTS so the device actually goes silent
                    // for n seconds instead of speaking "N seconds".
                    let _ = tts_tx.send(tts::TtsCommand::Pause(n));
                }
                LlmOutput::Bell => {
                    info!("bell");
                    app.push_sentence("[bell]".to_string());
                    let _ = tts_tx.send(tts::TtsCommand::Bell);
                }
                LlmOutput::Done => {
                    app.finish();
                    info!("generation complete, {} tokens", app.token_count);
                }
                LlmOutput::Error(e) => {
                    error!("LLM error: {e}");
                    app.push_console(format!("LLM Error: {e}"));
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
                KeyCode::Enter => handle_start(app, stt_tx),
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
/// - Idle: start listening via STT (mic -> transcribe -> LLM)
/// - Done: reset and return to idle for another session
/// - Generating/Paused: ignored (generation is already in progress)
fn handle_start(app: &mut App, stt_tx: &mpsc::Sender<stt::SttCommand>) {
    match app.state {
        // From Idle OR Done: start a fresh listen. After a meditation
        // finishes the user can press ENTER again to start a follow-up
        // turn — the previous meditation text is cleared so the new
        // session has a clean canvas.
        AppState::Idle | AppState::Done => {
            if app.state == AppState::Done {
                info!("starting new turn — clearing previous meditation");
                app.reset();
            }
            info!("starting STT listen");
            app.push_console("Listening...".to_string());
            let _ = stt_tx.send(stt::SttCommand::Listen);
        }
        AppState::Generating | AppState::Paused => {
            info!("start pressed during generation — ignored");
        }
        AppState::Loading => {
            info!("start pressed during model load — ignored");
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
