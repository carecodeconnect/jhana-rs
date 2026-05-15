# 17: Display Stack — Survey and Decision

Captured from a research agent run on 2026-05-15. The question:
**which graphical display stack should jhana-rs use** to get past the
Linux TTY console's inability to render unicode block characters as
sub-cell pixels (which broke `tui-big-text` rendering — see
`docs/14_TODO.md` task #20 and commit `cb642b4`)?

## Constraints

- Single-purpose appliance, one fullscreen surface at a time.
- ≤ 50 MB RSS budget for the display stack (LLM + STT + TTS use 5+ GB
  of the 8 GB on the Rock 5A).
- Rust-native or Rust-friendly preferred.
- Fast boot (≤ 90 s time-to-greeting, currently ~30-40 s).
- aarch64 + RK3588 (Mali-G610 Panfrost) compatible.
- No keyboard/mouse — only 3 GPIO buttons via `gpio-keys`.

## Per-candidate scorecard

| Candidate | RAM | Lang | Needs X/Wayland | tui-big-text | Hosts ratatui | Hosts Slint/egui | RK3588 risk |
|---|---|---|---|---|---|---|---|
| **kmscon** as `getty` replacement | ~15 MB | C | **no** | **yes** | yes (native) | no | low |
| cage + foot | 25-40 MB | C | self-Wayland | yes | yes | yes | low |
| weston + kiosk-shell | 30-50 MB | C | self-Wayland | yes | yes | yes | low |
| sway / wayfire kiosk | 40-80 MB | C/C++ | self-Wayland | yes | yes | yes | low |
| gamescope | 60+ MB | C++ | self-Wayland | yes | yes | yes | medium |
| Xorg + leftwm + alacritty | ~80 MB | Rust WM | X server | yes | yes | yes | low |
| Xorg + bare alacritty | ~70 MB | C/Rust | X server | yes | yes | yes | low |
| Slint with `linuxkms` backend | 10-30 MB | Rust | **none** | n/a | no terminal | native | medium (linuxkms tagged "experimental") |
| fbterm | ~5 MB | C | none | partial | yes | no | medium (unmaintained) |

## The surprising winner: kmscon

**[kmscon](https://en.wikipedia.org/wiki/Kmscon)** is a userspace VT
daemon that replaces the in-kernel framebuffer console. It renders
via DRM/KMS using FreeType, so it can load any TTF/OTF font instead
of the kernel's 512-glyph PSF table. Crucially: **unicode quadrant
block characters render as actual sub-cell glyphs**, which is exactly
what `tui-big-text` (and similar half-block tricks) need.

Project was dormant 2015-2025, **revived in 2025** (per Fedora's
[UseKmsconVTConsole change proposal](https://fedoraproject.org/wiki/Changes/UseKmsconVTConsole)
and the [kmscon Wikipedia article](https://en.wikipedia.org/wiki/Kmscon)).

**Drop-in install**: replace `getty@tty1.service` with
`kmsconvt@tty1.service` in the boot path. The `jhana-rs` binary keeps
running exactly as it does today; only the VT under it changes. Zero
Rust code changes. ~15 MB extra resident memory. DRM/KMS native so
Panfrost on Mali-G610 is fine.

This is the **one-evening fix** that unblocks the original
ratatui + tui-big-text design. We were one daemon away from a working
big-text meditation surface the whole time.

## Recommendations

### Phase A (ship-today, low-risk): kmscon — STAGED 2026-05-15

Implementation is **staged** in the repo, awaiting the next Rock-online
session for `apt install kmscon`:

- `hardware/jhana-rs-kmscon.service` — new systemd unit. Launches
  kmscon on `--vt=1` with `jhana-rs` as the login program. Conflicts
  with both `getty@tty1.service` and the previous `jhana-rs.service`,
  so enabling it cleanly takes over tty1.
- `hardware/kmscon.conf` — kmscon configuration. Uses `Monospace`
  fontconfig name (resolves to DejaVu Sans Mono on Ubuntu/Armbian)
  at size 24. `drm` + `hwaccel` for Panfrost-accelerated rendering.
  `no-switchvt` because the device has no Ctrl-Alt-F1..F6 hardware.
- `scripts/rock-kmscon-setup.sh` — one-shot installer. Run from the
  dev machine: `./scripts/rock-kmscon-setup.sh`. Idempotent.
  Rollback is `systemctl disable --now jhana-rs-kmscon.service &&
  systemctl enable --now jhana-rs.service`.
- `Cargo.toml` — `tui-big-text = "0.7.3"` re-added.
- `src/ui.rs` — `BigText::builder().pixel_size(PixelSize::Quadrant)`
  restored in `render_focal_card` and `render_loading_card`. Layout:
  6-row big-text band + 1-row gap + plain-text mirror, centred
  vertically with computed padding.

To deploy on the Rock:

```bash
scripts/rock-kmscon-setup.sh          # one shot, ~30 s
scripts/rock-log.sh -f                 # watch the new render path
```

Reverting (if kmscon misbehaves):

```bash
ssh ubuntu@rock-5a
sudo systemctl disable --now jhana-rs-kmscon.service
sudo systemctl enable  --now jhana-rs.service        # restores getty/Linux VT path
```

### Phase B (longer-term, optional): cage + foot, plan Slint for v2

If we want more than monospaced text — Pali diacritics rendered with
proper typography, a breath waveform, a mandala animation, mic-level
meter — we eventually outgrow ratatui. Path:

1. **Cage + foot** in Phase A.1 instead of kmscon. Marginally bigger
   footprint (~35 MB vs 15 MB) but gives us Wayland for free,
   meaning the foot terminal hosts the existing ratatui while we
   prototype a native UI alongside it.
2. **Slint with linuxkms backend** as a from-scratch UI rewrite,
   running directly on DRM/KMS without a compositor. Slint is
   Rust-native, designed for embedded Linux (multiple production
   deployments on Cortex-A class), uses GL ES on the Mali GPU.
   Estimated rewrite: 1-2 weeks for feature parity with the current
   ratatui surface, much more for the dream UI.

The `linuxkms` backend conflicts with cage (both hold DRM master),
so the migration is a swap, not a coexistence. Same systemd unit,
different binary.

### Not recommended

- **Xorg + anything** (including leftwm): biggest footprint, slowest
  boot, dragging in X11 for no real gain over Wayland. Skip unless
  there's a specific X-only tool we need.
- **gamescope**: optimised for games + nested compositing; unnecessary
  complexity for a single-app appliance.
- **gnome-kiosk**: pulls in Mutter + GTK plumbing, 100+ MB RSS.
- **Slint from-scratch as Phase A**: too much surgery for the
  ship-today milestone. Earn it via Phase B.

## Sources

Captured by the research agent; all verified URLs:

- [Cage Wayland kiosk](https://www.hjdskes.nl/projects/cage/) · [cage-kiosk/cage GitHub](https://github.com/cage-kiosk/cage)
- [Weston kiosk-shell docs](https://wayland.pages.freedesktop.org/weston/toc/kiosk-shell.html)
- [LeftWM](https://leftwm.org/) · [leftwm GitHub](https://github.com/leftwm/leftwm)
- [Slint LinuxKMS backend](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/) · [Slint embedded](https://docs.slint.dev/latest/docs/slint/guide/platforms/embedded/)
- [Mesa Panfrost](https://docs.mesa3d.org/drivers/panfrost.html) · [Mali-G610 OpenGL ES 3.1 conformance (Collabora)](https://www.collabora.com/news-and-blog/news-and-events/taming-the-panthor-opengl-es-31-conformance-achived-mali-g610.html) · [PanVK Vulkan 1.2 (Collabora)](https://www.collabora.com/news-and-blog/news-and-events/panvk-reaches-vulkan-12-conformance-on-mali-g610.html)
- [Radxa Mali GPU docs](https://docs.radxa.com/en/rock5/rock5c/radxa-os/mali-gpu)
- [KMSCON ArchWiki](https://wiki.archlinux.org/title/KMSCON) · [kmscon Wikipedia (revival 2025)](https://en.wikipedia.org/wiki/Kmscon) · [Fedora UseKmsconVTConsole](https://fedoraproject.org/wiki/Changes/UseKmsconVTConsole)
- [foot terminal](https://codeberg.org/dnkl/foot)
