//! `jhana-slint` — Phase C variant of the meditation app.
//!
//! Same agent_loop / LLM / STT / TTS modules as the main `jhana-rs`
//! binary, but the UI is **Slint** rendering directly to DRM/KMS via
//! the `linuxkms` backend. No kmscon, no compositor, no Linux VT.
//!
//! Aesthetic: Tron / WarGames / Weird Science (see
//! `docs/17_DISPLAY.md` § Visual direction and `docs/18_SLINT.md`).
//!
//! C-3 milestone: live AgentEvent → Slint property bridge. Real
//! meditation flow runs and updates the UI in real time.

// Pull in the existing crate modules via `#[path]` includes so the
// slint binary uses the same agent loop / model / audio / GPIO code
// as the main `jhana-rs` binary. (Long-term we'd extract these to a
// proper lib crate; for now this matches the pattern used by
// `jhana-llm-server.rs`.)
#[path = "../agent_loop.rs"]
mod agent_loop;
#[path = "../config.rs"]
mod config;
#[path = "../gpio.rs"]
mod gpio;
#[path = "../llm.rs"]
mod llm;
#[path = "../openai_types.rs"]
mod openai_types;
#[path = "../stt.rs"]
mod stt;
#[path = "../tts.rs"]
mod tts;

// Pull in the generated Slint code (compiled from `ui/jhana.slint`
// by `slint-build` in `build.rs`).
slint::include_modules!();

use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::{error, info};

use crate::agent_loop::{AgentCtx, AgentEvent};
use crate::gpio::ButtonEvent;
use crate::openai_types::ChatMessage;

/// Build the seed history for an agent session. Mirrors the version
/// in `src/main.rs` (which is private to that binary). When we
/// eventually extract a shared lib crate, this gets removed.
fn seed_agent_history() -> Vec<ChatMessage> {
    let system = std::fs::read_to_string("prompts/agent_system.txt").unwrap_or_else(|e| {
        error!("could not read prompts/agent_system.txt: {e} — using minimal fallback");
        "You are Jhana, a meditation guide. Speak only by emitting \
         <tool_call>{...}</tool_call> blocks. Open with say(\"Hello?\") \
         then listen(). End every session with goodnight()."
            .to_string()
    });
    vec![
        ChatMessage::system(system),
        ChatMessage::user("[User pressed the ENTER button to begin.]"),
    ]
}

/// Spawn an agent session in a background thread.
fn spawn_agent_session(
    tts_tx: mpsc::Sender<tts::TtsCommand>,
    stt_tx: mpsc::Sender<stt::SttCommand>,
    stt_results: Arc<Mutex<mpsc::Receiver<stt::SttResult>>>,
    agent_tx: mpsc::Sender<AgentEvent>,
    cancel: Arc<AtomicBool>,
) {
    cancel.store(false, Ordering::Release);
    std::thread::Builder::new()
        .name("agent".into())
        .spawn(move || {
            let handle = match crate::llm::get_or_load_model() {
                Ok(h) => h,
                Err(e) => {
                    error!("agent: model load failed: {e}");
                    let _ = agent_tx.send(AgentEvent::Error(e));
                    return;
                }
            };
            let ctx = AgentCtx {
                tts_tx,
                stt_tx,
                stt_results,
                skills_dir: std::path::PathBuf::from("prompts/agent_skills"),
            };
            let mut history = seed_agent_history();
            if let Err(e) = crate::agent_loop::run_agent(
                handle,
                &mut history,
                &agent_tx,
                &ctx,
                &cancel,
                15, // max_turns
            ) {
                match e {
                    crate::agent_loop::AgentError::Cancelled => {
                        info!("agent: cancelled");
                        let _ = agent_tx.send(AgentEvent::Done);
                    }
                    other => {
                        error!("agent: {other}");
                        let _ = agent_tx.send(AgentEvent::Error(other.to_string()));
                    }
                }
            }
        })
        .expect("failed to spawn agent thread");
}

/// One-line summary of tool-call args for the activity log.
fn short_args(v: &serde_json::Value) -> String {
    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        if text.len() > 60 {
            format!("\"{}…\"", &text[..60])
        } else {
            format!("\"{text}\"")
        }
    } else if let Some(secs) = v.get("seconds") {
        format!("{secs}s")
    } else if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
        format!("\"{name}\"")
    } else {
        "".to_string()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // File logger (same path as the main binary so rock-log.sh works).
    let log_file = std::fs::File::create("jhana-rs.log")?;
    simplelog::WriteLogger::init(
        log::LevelFilter::Info,
        simplelog::Config::default(),
        log_file,
    )
    .ok();
    info!("jhana-slint starting");

    // GPIO buttons (returns None on non-Rock hardware).
    let button_rx = crate::gpio::start_polling();
    if button_rx.is_some() {
        info!("GPIO buttons enabled");
    }

    // Audio: TTS thread (spawn before STT so we can hand a clone to STT
    // for the cue-routing trick).
    let tts_tx = crate::tts::start();

    // Audio: STT thread. Wrap result rx in Arc<Mutex> for cross-session
    // sharing between the main (UI) thread and the agent thread.
    let (stt_result_tx, stt_result_rx) = mpsc::channel::<stt::SttResult>();
    let stt_tx = crate::stt::start(stt_result_tx, tts_tx.clone());
    let stt_result_rx = Arc::new(Mutex::new(stt_result_rx));

    // Pre-load the RKLLM model in the background.
    crate::llm::preload();

    // AgentEvent channel — the agent thread sends here; the Slint
    // event loop polls this on a Timer.
    let (agent_tx, agent_rx) = mpsc::channel::<AgentEvent>();
    let agent_rx = Arc::new(Mutex::new(agent_rx));
    let cancel = Arc::new(AtomicBool::new(false));

    // Build the Slint window. Pulled from generated code in ui/jhana.slint.
    let main_window = MainWindow::new()?;
    main_window.set_state("loading".into());
    main_window.set_active_tool("".into());
    main_window.set_current_say("warming the meditation model".into());
    main_window.set_pause_remaining(0);

    // Slint models live on the main thread — Rc, not Arc. The agent
    // thread updates the log indirectly by sending AgentEvent through
    // the channel; the Timer callback (main thread) consumes events
    // and pushes to this model.
    let log_model: Rc<slint::VecModel<LogEntry>> = Rc::new(slint::VecModel::default());
    main_window.set_log_entries(slint::ModelRc::from(log_model.clone()));

    // Track an "active state" string we mutate from event handlers.
    // Stored in an RwLock so the Timer can read+write without locking
    // the whole window.
    let session_running = Arc::new(AtomicBool::new(false));

    // Periodic event-pump Timer (≈30 Hz). Drains AgentEvent +
    // ButtonEvent, mutates Slint properties.
    let event_timer = slint::Timer::default();
    let weak = main_window.as_weak();
    let log_model_for_timer = log_model.clone();
    let agent_tx_for_timer = agent_tx.clone();
    let tts_tx_for_timer = tts_tx.clone();
    let stt_tx_for_timer = stt_tx.clone();
    let stt_results_for_timer = stt_result_rx.clone();
    let cancel_for_timer = cancel.clone();
    let session_running_for_timer = session_running.clone();
    let button_rx_for_timer = button_rx;
    let agent_rx_for_timer = agent_rx.clone();

    event_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(33),
        move || {
            let Some(window) = weak.upgrade() else { return };

            // Transition out of "loading" once both models report ready.
            if window.get_state() == slint::SharedString::from("loading")
                && crate::stt::STT_READY.load(Ordering::Acquire)
                && crate::llm::LLM_READY.load(Ordering::Acquire)
            {
                window.set_state("idle".into());
                window.set_current_say("press enter to begin".into());
            }

            // Drain GPIO button events (ENTER → start agent session, BACK → quit).
            if let Some(rx) = &button_rx_for_timer {
                while let Ok(event) = rx.try_recv() {
                    info!("button: {event:?}");
                    match event {
                        ButtonEvent::Back => {
                            cancel_for_timer.store(true, Ordering::Release);
                            slint::quit_event_loop().ok();
                        }
                        ButtonEvent::Enter => {
                            if !session_running_for_timer.load(Ordering::Acquire) {
                                session_running_for_timer.store(true, Ordering::Release);
                                window.set_state("thinking".into());
                                spawn_agent_session(
                                    tts_tx_for_timer.clone(),
                                    stt_tx_for_timer.clone(),
                                    stt_results_for_timer.clone(),
                                    agent_tx_for_timer.clone(),
                                    cancel_for_timer.clone(),
                                );
                            }
                        }
                        ButtonEvent::Up | ButtonEvent::Down => {
                            // Log scrolling — not implemented in C-3.
                        }
                    }
                }
            }

            // Drain AgentEvents and update Slint props.
            let Ok(rx_guard) = agent_rx_for_timer.try_lock() else {
                return;
            };
            while let Ok(event) = rx_guard.try_recv() {
                match event {
                    AgentEvent::Sentence(_) => {
                        // Raw token stream — discarded for the Slint UI.
                        // The ToolStart/ToolResult events carry the
                        // human-meaningful content.
                    }
                    AgentEvent::ToolStart { name, args } => {
                        info!("agent tool start: {name}({args})");
                        log_model_for_timer.push(LogEntry {
                            kind: "tool".into(),
                            text: format!("{name}({})", short_args(&args)).into(),
                        });
                        match name.as_str() {
                            "say" => {
                                if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                                    window.set_current_say(text.into());
                                    log_model_for_timer.push(LogEntry {
                                        kind: "agent".into(),
                                        text: text.into(),
                                    });
                                }
                                window.set_active_tool("speaking".into());
                            }
                            "listen" => {
                                window.set_active_tool("listening".into());
                            }
                            "pause" => {
                                let secs =
                                    args.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as i32;
                                window.set_active_tool("pausing".into());
                                window.set_pause_remaining(secs);
                            }
                            "ring_bell" => {
                                window.set_active_tool("ringing".into());
                            }
                            _ => {}
                        }
                    }
                    AgentEvent::ToolResult { name, ok, snippet } => {
                        info!("agent tool result: {name} ok={ok} {snippet}");
                        window.set_active_tool("".into());
                        if name == "listen" && ok {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&snippet)
                                && let Some(t) = v.get("transcript").and_then(|t| t.as_str())
                            {
                                log_model_for_timer.push(LogEntry {
                                    kind: "user".into(),
                                    text: t.into(),
                                });
                            }
                        }
                    }
                    AgentEvent::Done => {
                        info!("agent session done");
                        window.set_state("done".into());
                        window.set_active_tool("".into());
                        session_running_for_timer.store(false, Ordering::Release);
                    }
                    AgentEvent::Error(e) => {
                        error!("agent error: {e}");
                        log_model_for_timer.push(LogEntry {
                            kind: "system".into(),
                            text: format!("error: {e}").into(),
                        });
                        window.set_state("done".into());
                        session_running_for_timer.store(false, Ordering::Release);
                    }
                }
            }
        },
    );

    info!("Slint event loop starting");
    main_window.run()?;
    info!("jhana-slint exiting");
    Ok(())
}
