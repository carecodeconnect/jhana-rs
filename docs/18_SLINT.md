# 18: Phase C — Slint UI rewrite (Tron/WarGames terminal)

Concrete scope for replacing ratatui + kmscon with a native Slint
GUI rendering directly to DRM/KMS via the `linuxkms` backend. The
agent loop, LLM, STT, TTS, GPIO, config modules are **unchanged** —
only the UI layer (`src/ui.rs` and the main event loop draw path)
gets rewritten.

Cross-link: aesthetic spec in `docs/17_DISPLAY.md` § "Visual
direction" (Tron / WarGames / Weird Science).

## Why Slint

- **Pure Rust** — fits the rest of the stack. No JNI, no Python, no
  web view, no C++ FFI beyond what the renderer itself uses.
- **Embedded-first** — Slint ships explicit `linuxkms` and `software`
  renderers for SBCs. Multiple production deployments on Cortex-A.
- **No compositor** — `linuxkms` backend renders directly via
  DRM/KMS. We don't need Wayland or X, and we don't need kmscon
  either (the Slint process IS the DRM master).
- **GL ES on the Mali G610** — Slint's femtovg renderer uses
  OpenGL ES; Panfrost provides GL ES 3.1 conformance on RK3588 per
  Collabora. Software renderer is a safe fallback if Panfrost
  misbehaves.
- **Custom fonts at any size** — TTF/OTF/bitmap fonts load via
  Slint's font asset pipeline. Pixel fonts (Press Start 2P, VT323)
  work natively.
- **Glow / alpha / vector shapes** — Slint draws via skia or
  femtovg; alpha blending, drop shadows, rounded rects, paths, and
  animations are all first-class.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ jhana-rs-slint binary                                        │
│                                                              │
│   src/bin/jhana-slint.rs ──┐                                 │
│                            │                                 │
│   src/ui_slint.rs ─────────┼── Slint event loop, property    │
│                            │   bindings, glue to AgentEvent  │
│                            │                                 │
│   ui/jhana.slint ──────────┘   declarative UI (.slint DSL)   │
│                                compiled at build time via    │
│                                slint-build in build.rs       │
│                                                              │
│   modules (unchanged):                                       │
│     agent_loop.rs · llm.rs · stt.rs · tts.rs · gpio.rs       │
│     config.rs · openai_types.rs                              │
└──────────────────────────────────────────────────────────────┘
            ▲                                ▲
            │ AgentEvent                     │ DRM /dev/dri/card1
            │ (mpsc)                         │ (linuxkms backend)
            │                                ▼
   agent thread (unchanged)         RK3588 / Mali G610 / Panfrost
                                    direct kernel mode-set
```

## Crate layout

- **Keep `jhana-rs` binary** (`src/main.rs`) — ratatui + kmscon
  hosted. Stays the production default until Slint passes UAT.
- **New `jhana-rs-slint` binary** — `src/bin/jhana-slint.rs`
  (alongside `qwen-tool-test.rs` and `jhana-llm-server.rs`).
- **New module `src/ui_slint.rs`** — the Slint event-loop integration
  + AgentEvent → Slint property bridge.
- **New `ui/jhana.slint`** — the declarative UI file.
- **New `build.rs`** — invokes `slint-build` to compile `ui/jhana.slint`
  into generated Rust code referenced by `src/ui_slint.rs`.

## Cargo dependencies

```toml
slint = { version = "1", default-features = false, features = [
    "backend-linuxkms",      # render to DRM/KMS directly, no compositor
    "renderer-software",     # CPU rasteriser — safe fallback
    "renderer-femtovg",      # GL ES via Panfrost — preferred on RK3588
    "compat-1-2",            # 1.x compatibility shims
] }

[build-dependencies]
slint-build = "1"
```

Approximate cost: ~6-8 MB extra binary size, ~5 MB extra build
dependencies. Compile time grows ~30 s for the first build.

## Systemd unit

```ini
[Unit]
Description=jhana-rs (Slint linuxkms variant)
After=multi-user.target sound.target
Conflicts=getty@tty1.service jhana-rs-kmscon.service jhana-rs.service

[Service]
Type=simple
User=root
WorkingDirectory=/home/ubuntu/jhana-rs
ExecStart=/home/ubuntu/jhana-rs/target/release/jhana-rs-slint
Environment=SLINT_BACKEND=linuxkms
Environment=RUST_LOG=info
StandardInput=null
StandardOutput=journal
StandardError=journal
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Note: no kmscon, no getty, no Linux VT. Slint with linuxkms backend
takes the DRM master directly.

## UI surface (`ui/jhana.slint`)

Three regions, mirroring the ratatui layout but with real graphics:

1. **Status bar** (top, 60 px) — Tron-grid background, neon-coloured
   state pill (idle/listening/speaking/pausing/thinking). State name
   in pixel font.
2. **Focal card** (middle, 600 px) — black with cyan neon double-
   border. Content depends on `active_tool`:
   - `say(text)`: big text fades in word-by-word, cyan glow.
   - `listen()`: animated "LISTENING" with bouncing dots + recording
     amplitude bar.
   - `pause(n)`: large countdown number in green phosphor, ticking
     down once per second.
   - `ring_bell()`: bell glyph flashes magenta then fades.
   - idle: a static mandala or ensō circle, slowly rotating.
3. **Activity log** (bottom, 620 px) — terminal-style green-on-black
   scrolling log. Each line prefixed by `> ` for user, `JHANA: ` for
   agent, dim for tool calls. Older lines fade toward dark.

Background: full-frame Tron grid drawn in dim cyan (`#0e1a26`), 1 px
scanline overlay at 8 % opacity.

## AgentEvent → Slint bridge

The agent thread continues to send `AgentEvent` over an `mpsc::Sender`.
The Slint main thread polls the receiver from a `Timer` (Slint's
periodic callback) at 30 fps and updates Slint properties via the
generated `MainWindow::set_*` methods. Concretely:

```rust
let weak = main_window.as_weak();
slint::Timer::default().start(
    slint::TimerMode::Repeated,
    std::time::Duration::from_millis(33),
    move || {
        let Some(window) = weak.upgrade() else { return };
        while let Ok(event) = agent_rx.try_recv() {
            match event {
                AgentEvent::ToolStart { name, args } if name == "say" => {
                    if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                        window.set_current_say(text.into());
                        window.set_active_tool("speaking".into());
                    }
                }
                // ... other arms ...
            }
        }
    },
);
```

The Slint event loop owns the main thread; agent / STT / TTS run in
their existing background threads. `Sender<AgentEvent>` is `Send`,
so the cross-thread plumbing matches what `main.rs` already has.

## Cancellation / shutdown

Currently the BACK button flips `cancel: Arc<AtomicBool>` and the
agent thread polls it. Same here — GPIO button thread keeps writing
to the same cancel flag, the agent honours it, no changes needed.

For Slint window close: Slint's `quit_event_loop()` is wired to the
quit signal as well. Both routes call into the same shutdown path.

## Implementation phases (within Phase C)

- **C-1 — Scaffold.** Add deps, build.rs, empty `ui/jhana.slint`
  showing just a centred "loading" text on black. Empty
  `src/bin/jhana-slint.rs` that opens the window. Verify
  `cargo build --bin jhana-rs-slint` on the Rock.
- **C-2 — Static layout.** Implement the three regions (status bar /
  focal card / activity log) with hardcoded sample content. Pixel
  font loaded. Tron grid background. No animations yet.
- **C-3 — Live binding.** Wire AgentEvent into Slint properties.
  Status bar reflects state. Focal card shows say() text. Log scrolls.
  Verify on the Rock with `harness=agent`.
- **C-4 — Animations.** Word-by-word reveal, pause countdown, bell
  flash, idle mandala rotation, scanline overlay.
- **C-5 — Systemd swap.** Disable kmscon path, enable slint path,
  reboot, verify boot-to-meditation in under 90 s.
- **C-6 — Decoration pass.** Glow effects on focal text. Tron grid
  refinement. Ensō / monk silhouette / candle flame sprites.

C-1 through C-3 are the working-MVP. C-4 onward is polish.

## Fallback

If Slint's linuxkms backend turns out to be too fragile on RK3588 +
Panfrost, the ratatui+kmscon path (`jhana-rs-kmscon.service`) stays
fully functional. Reverting is `systemctl disable --now
jhana-rs-slint.service && systemctl enable --now
jhana-rs-kmscon.service`. The Slint binary just sits unused.

## Open questions

- Slint's font asset pipeline: do we embed the pixel font in the
  binary, or load from disk at runtime? Embedding is simpler.
- DRM seat permissions: does Slint need `systemd-logind` / `seatd`,
  or can it run as root and grab DRM master directly? linuxkms
  examples I've seen use root + suid; we already run as root.
- Frame rate: 30 fps is overkill for meditation (eased animations
  read better at 24 fps cinematic). Tune in C-4.
- HDMI vs DSI: Slint's linuxkms picks a connector automatically. If
  it picks HDMI-A-1 (currently disconnected) instead of DSI-1, we'll
  need to nudge it via `SLINT_KMS_DEVICE` env or a connector hint.
