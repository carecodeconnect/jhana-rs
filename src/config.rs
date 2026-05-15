//! Runtime configuration loaded from `config/jhana.json`.
//!
//! Keeping the tunable values out of `const` declarations means we can
//! swap LLM models, change audio routing, tweak espeak parameters,
//! re-word the welcome speech, etc. without recompiling. The schema is
//! deliberately flat and forgiving — missing optional fields fall back
//! to sensible defaults so an old `jhana.json` keeps working when we
//! add new knobs.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

/// Absolute or relative path to the live config. Override with the
/// `JHANA_CONFIG` env var (used by tests + by `scripts/rock-run.sh`
/// when we want to point at a per-environment file).
const DEFAULT_CONFIG_PATH: &str = "config/jhana.json";

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub struct Config {
    pub active_model: String,
    pub models: HashMap<String, ModelConfig>,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub path: String,
    pub max_context_len: i32,
    pub max_new_tokens: i32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub repeat_penalty: f32,
    #[serde(default)]
    #[expect(dead_code)] // documentation field, not consumed by code
    pub notes: String,
}

#[derive(Debug, Deserialize)]
pub struct AudioConfig {
    pub pulse_server: String,
    pub speaker_sink: String,
    pub mic_source: String,
    pub capture_format: String,
    pub capture_rate: u32,
    pub record_seconds: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            pulse_server: "unix:/var/run/pulse/native".to_string(),
            speaker_sink: "alsa_output.platform-uctronics-sound.stereo-fallback".to_string(),
            mic_source: "alsa_input.platform-uctronics-sound.stereo-fallback".to_string(),
            capture_format: "S32_LE".to_string(),
            capture_rate: 48_000,
            record_seconds: 5,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TtsConfig {
    pub engine: String,
    pub espeak_amplitude: u32,
    pub espeak_rate: u32,
    #[serde(default)]
    pub paroli: Option<ParoliConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParoliConfig {
    pub bin: String,
    pub encoder: String,
    pub decoder: String,
    pub config: String,
    pub espeak_data: String,
    #[serde(default)]
    pub ld_library_path: String,
    #[serde(default = "default_length_scale")]
    pub length_scale: f32,
    #[serde(default)]
    #[expect(dead_code)] // documentation field, not consumed by code
    pub notes: String,
}

fn default_length_scale() -> f32 {
    1.0
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            engine: "espeak-ng".to_string(),
            espeak_amplitude: 100,
            espeak_rate: 145,
            paroli: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UiConfig {
    pub default_meditation: String,
    pub welcome_lines: Vec<String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            default_meditation: "lotus_flower".to_string(),
            welcome_lines: vec![
                "Welcome to jhana-rs.".to_string(),
                "Press the enter button to begin a meditation.".to_string(),
                "Press back to quit.".to_string(),
            ],
        }
    }
}

/// Read `config/jhana.json` (or `$JHANA_CONFIG`) once per process and
/// return a static reference. Panics on missing/malformed file —
/// startup is the right time to fail loudly so the user gets a clear
/// error instead of a silent fallback.
pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| {
        let path = std::env::var("JHANA_CONFIG").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());
        let path_buf = PathBuf::from(&path);
        let raw = fs::read_to_string(&path_buf).unwrap_or_else(|e| {
            panic!("failed to read {path_buf:?}: {e} (set JHANA_CONFIG or copy config/jhana.json)")
        });
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("malformed {path_buf:?}: {e}"))
    })
}

/// Convenience: fetch the active model's config.
pub fn active_model() -> &'static ModelConfig {
    let cfg = get();
    cfg.models
        .get(&cfg.active_model)
        .unwrap_or_else(|| panic!("active_model '{}' not in models table", cfg.active_model))
}
