//! Text-to-speech with paroli (streaming Piper) as the primary engine
//! and espeak-ng as a robotic fallback.
//!
//! Runs TTS on a background thread, receiving sentences via an mpsc
//! channel. Each sentence is synthesised to a WAV file (via paroli-cli
//! or espeak-ng depending on `config/jhana.json` → `tts.engine`) and
//! played through PulseAudio on the Uctronics speaker (system-mode
//! PA, sink alsa_output.platform-uctronics-sound.stereo-fallback).
//!
//! Pause + bell tools (LlmOutput::Pause / Bell) are also handled here:
//! Pause = `std::thread::sleep`, Bell = `paplay` of a pre-rendered
//! chime WAV.
//!
//! # Voice swap
//!
//! Edit `config/jhana.json` → `tts.paroli.encoder/decoder/config`
//! to point at a different streaming-piper voice (see
//! [marty1885/streaming-piper](https://huggingface.co/marty1885/streaming-piper))
//! and restart `jhana-rs.service`. No recompile.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};

use log::{error, info};

use crate::config;

/// Long-lived Moonshine worker subprocess. We spawn it once at TTS
/// thread startup if the engine is `"moonshine"`, then write one JSON
/// line per request to its stdin and read one JSON line back. Keeps
/// the Kokoro voice loaded across utterances (~3 s load is paid once).
struct MoonshineWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

static MOONSHINE_WORKER: Mutex<Option<MoonshineWorker>> = Mutex::new(None);

/// Messages sent to the TTS thread.
#[derive(Debug)]
pub enum TtsCommand {
    /// Speak this sentence aloud.
    Speak(String),
    /// Silent pause for this many seconds. Honoured by the TTS thread
    /// (it sleeps) so the user actually experiences the pause instead
    /// of hearing "fifteen seconds" spoken aloud.
    Pause(f32),
    /// Ring the meditation bell.
    Bell,
    /// Stop any pending speech (flush the queue).
    #[expect(dead_code)] // will be used when cancel-generation is wired
    Stop,
}

/// PulseAudio system-mode socket (set up by pulseaudio.service on the
/// Rock). We use PA rather than raw `aplay` so we get software mixing,
/// the original baseline's 100 %-sink-volume loudness path, and no
/// per-utterance ALSA open (which on the Uctronics codec triggers a
/// speaker-amp pop every time).
const PULSE_SERVER: &str = "unix:/var/run/pulse/native";

/// PulseAudio sink the Uctronics speaker exposes.
const PULSE_SINK: &str = "alsa_output.platform-uctronics-sound.stereo-fallback";

/// Temporary WAV file for TTS output.
const WAV_PATH: &str = "/tmp/jhana_tts.wav";

/// Pre-rendered meditation-bell WAV.
const BELL_WAV: &str = "/tmp/jhana_bell.wav";

/// Start the TTS background thread. Returns a sender for sentences /
/// pauses / bell commands. The thread loads the configured engine
/// once (paroli or espeak-ng) and serialises all playback.
pub fn start() -> Sender<TtsCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<TtsCommand>();

    std::thread::Builder::new()
        .name("tts".into())
        .spawn(move || {
            let cfg = &config::get().tts;
            info!("TTS thread started (engine={})", cfg.engine);
            tts_loop(&rx);
            info!("TTS thread exiting");
        })
        .expect("failed to spawn TTS thread");

    tx
}

/// TTS event loop — processes commands until the channel closes.
fn tts_loop(rx: &Receiver<TtsCommand>) {
    // Availability check for the configured engine. We don't bail if
    // the primary engine is missing — the speak path retries via the
    // espeak-ng fallback per sentence, which is reasonable until we
    // remove espeak-ng entirely.
    if Command::new("espeak-ng").arg("--version").output().is_ok() {
        info!("espeak-ng available (fallback)");
    } else {
        error!("espeak-ng not found — TTS will be silent if paroli fails");
    }

    // Pre-render the meditation bell once.
    render_bell();

    // If the configured engine is Moonshine, spawn the persistent
    // Python worker now so the first sentence doesn't pay the 3 s
    // voice-load cost.
    if config::get().tts.engine == "moonshine" {
        spawn_moonshine_worker();
    }

    while let Ok(cmd) = rx.recv() {
        match cmd {
            TtsCommand::Speak(sentence) => speak_sentence(&sentence),
            TtsCommand::Pause(seconds) => {
                let clamped = seconds.clamp(0.0, 120.0);
                info!("TTS: pausing {clamped:.1}s");
                std::thread::sleep(std::time::Duration::from_millis(
                    (clamped * 1000.0) as u64,
                ));
            }
            TtsCommand::Bell => {
                info!("TTS: ring bell");
                play_wav(BELL_WAV);
            }
            TtsCommand::Stop => {
                info!("TTS stop requested");
                while rx.try_recv().is_ok() {}
            }
        }
    }
}

/// Synthesise `sentence` to WAV via the configured engine, then play
/// through PA. Falls back to espeak-ng if the primary engine fails.
fn speak_sentence(sentence: &str) {
    let preview = &sentence[..sentence.len().min(50)];
    let start = std::time::Instant::now();

    let cfg = &config::get().tts;
    let mut synthesised = false;

    match cfg.engine.as_str() {
        "paroli" if cfg.paroli.is_some() => {
            synthesised = synth_with_paroli(sentence);
            if synthesised {
                info!(
                    "TTS (paroli): {:.2}s — {preview}",
                    start.elapsed().as_secs_f32()
                );
            }
        }
        "moonshine" if cfg.moonshine.is_some() => {
            synthesised = synth_with_moonshine(sentence);
            if synthesised {
                info!(
                    "TTS (moonshine): {:.2}s — {preview}",
                    start.elapsed().as_secs_f32()
                );
            }
        }
        "espeak-ng" => {}
        other => {
            error!("Unknown tts.engine '{other}' — falling back to espeak");
        }
    }

    if !synthesised {
        if synth_with_espeak(sentence) {
            info!(
                "TTS (espeak): {:.2}s — {preview}",
                start.elapsed().as_secs_f32()
            );
        } else {
            error!("TTS failed (all engines) for: {preview}");
            return;
        }
    }

    play_wav(WAV_PATH);
}

/// Spawn the persistent Moonshine worker subprocess.
fn spawn_moonshine_worker() {
    let cfg = match &config::get().tts.moonshine {
        Some(m) => m,
        None => {
            error!("tts.engine = 'moonshine' but tts.moonshine block missing");
            return;
        }
    };

    info!("starting Moonshine worker (voice={})", cfg.voice);
    // jhana-rs.service runs as root, but the moonshine-voice assets
    // are cached under /home/ubuntu/.cache (where we did the pip
    // install + voice download). Without HOME override, the worker
    // silently re-downloads into /root/.cache and the TTS thread
    // blocks on the ready handshake. Pin HOME to the install user.
    let mut child = match Command::new(&cfg.python)
        .args([
            "-u", // unbuffered stdout — without this Python block-buffers
            // when stdout is a pipe, so our ready-line handshake never
            // arrives and the TTS thread hangs forever.
            &cfg.script,
            "--voice",
            &cfg.voice,
            "--language",
            &cfg.language,
        ])
        .env("HOME", "/home/ubuntu")
        .env("XDG_CACHE_HOME", "/home/ubuntu/.cache")
        .env("PYTHONUNBUFFERED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Capture stderr to a file so silent crashes leave evidence.
        // The path is constant so subsequent worker spawns overwrite
        // it; tail it with `tail -f /tmp/moonshine_worker.err`.
        .stderr(
            std::fs::File::create("/tmp/moonshine_worker.err")
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null()),
        )
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to spawn moonshine worker: {e}");
            return;
        }
    };

    let stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            error!("moonshine worker: no stdin");
            return;
        }
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            error!("moonshine worker: no stdout");
            return;
        }
    };
    let mut stdout = BufReader::new(stdout);

    // Wait for the ready handshake line.
    let mut ready_line = String::new();
    match stdout.read_line(&mut ready_line) {
        Ok(_) => info!("moonshine worker ready: {}", ready_line.trim()),
        Err(e) => {
            error!("moonshine worker readline failed: {e}");
            return;
        }
    }

    *MOONSHINE_WORKER.lock().unwrap() = Some(MoonshineWorker { child, stdin, stdout });
}

/// Synthesise via the persistent Moonshine worker. Writes the WAV at WAV_PATH.
fn synth_with_moonshine(sentence: &str) -> bool {
    let mut guard = MOONSHINE_WORKER.lock().unwrap();
    let w = match guard.as_mut() {
        Some(w) => w,
        None => {
            error!("moonshine worker not running");
            return false;
        }
    };

    // Escape text for JSON; build request line.
    let req = serde_json::json!({ "text": sentence, "out": WAV_PATH });
    if let Err(e) = writeln!(w.stdin, "{req}") {
        error!("moonshine worker write failed: {e}");
        return false;
    }
    if let Err(e) = w.stdin.flush() {
        error!("moonshine worker flush failed: {e}");
        return false;
    }

    let mut resp = String::new();
    if let Err(e) = w.stdout.read_line(&mut resp) {
        error!("moonshine worker read failed: {e}");
        return false;
    }
    match serde_json::from_str::<serde_json::Value>(resp.trim()) {
        Ok(v) if v.get("ok").and_then(|b| b.as_bool()) == Some(true) => true,
        Ok(v) => {
            error!("moonshine worker error: {}", v.get("error").map_or("?", |x| x.as_str().unwrap_or("?")));
            false
        }
        Err(e) => {
            error!("moonshine bad response: {e} (raw: {})", resp.trim());
            false
        }
    }
}

/// Run paroli-cli over `sentence`, writing the WAV at `WAV_PATH`.
fn synth_with_paroli(sentence: &str) -> bool {
    let cfg = match &config::get().tts.paroli {
        Some(p) => p,
        None => return false,
    };

    let result = Command::new(&cfg.bin)
        .env("LD_LIBRARY_PATH", &cfg.ld_library_path)
        .args([
            "--encoder",
            &cfg.encoder,
            "--decoder",
            &cfg.decoder,
            "-c",
            &cfg.config,
            "--espeak_data",
            &cfg.espeak_data,
            "--length_scale",
            &cfg.length_scale.to_string(),
            "--output_file",
            WAV_PATH,
            "--quiet",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(sentence.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            child.wait_with_output()
        });

    match result {
        Ok(out) if out.status.success() => std::fs::metadata(WAV_PATH)
            .map(|m| m.len() > 44)
            .unwrap_or(false),
        Ok(out) => {
            error!(
                "paroli failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            false
        }
        Err(e) => {
            error!("paroli error: {e}");
            false
        }
    }
}

/// Robotic fallback via espeak-ng.
fn synth_with_espeak(sentence: &str) -> bool {
    let cfg = &config::get().tts;
    Command::new("espeak-ng")
        .args([
            "-a",
            &cfg.espeak_amplitude.to_string(),
            "-s",
            &cfg.espeak_rate.to_string(),
            "-w",
            WAV_PATH,
            sentence,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Generate a meditation-bell WAV at startup.
fn render_bell() {
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f", "lavfi", "-i", "sine=frequency=523:duration=4",
            "-f", "lavfi", "-i", "sine=frequency=1046:duration=4",
            "-f", "lavfi", "-i", "sine=frequency=784:duration=4",
            "-filter_complex",
            "[0:a]volume=0.85,afade=t=out:st=0.05:d=3.9[a0];\
             [1:a]volume=0.35,afade=t=out:st=0.02:d=1.5[a1];\
             [2:a]volume=0.25,afade=t=out:st=0.03:d=2.0[a2];\
             [a0][a1][a2]amix=inputs=3:duration=longest:normalize=0",
            "-ar", "48000",
            "-ac", "1",
            "-sample_fmt", "s16",
            BELL_WAV,
        ])
        .status();
    match result {
        Ok(s) if s.success() => info!("bell rendered at {BELL_WAV}"),
        Ok(s) => error!("bell render failed: {s}"),
        Err(e) => error!("bell render error: {e}"),
    }
}

/// Play any pre-rendered WAV through the configured PA sink.
fn play_wav(path: &str) {
    match Command::new("paplay")
        .env("PULSE_SERVER", PULSE_SERVER)
        .args(["--device", PULSE_SINK, path])
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => error!("paplay failed: {}", String::from_utf8_lossy(&output.stderr)),
        Err(e) => error!("paplay error: {e}"),
    }
}
