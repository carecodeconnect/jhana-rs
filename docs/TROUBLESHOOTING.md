# Troubleshooting

## Rock 5A loses network / no SSH after reboot

Symptom: kernel reaches console (TTY shows boot messages or
ubuntu/armbian banners), but `ssh`/`ping` to `rock-5a` time out and
Tailscale shows `offline, last seen Nm ago`. `aplay -l` from a recovery
shell (if you can get one) usually still lists the cards, but systemd
never finishes userspace init.

This has happened repeatedly when changing the **uctronics-audio**
device-tree overlay. Recovery in every case: pull the microSD, mount
on dev machine, edit `/boot/armbianEnv.txt` to remove `uctronics-audio`
from `overlays=`, save, put back, reboot.

See [09_AUDIO.md](09_AUDIO.md) "DT overlay breaks networking" and
[SKILLS.md](SKILLS.md) "Mount the Rock's microSD on the dev machine".

### Confirmed-bad overlay variants (do not redeploy these)

These are the changes that have hung Armbian boot to date — keep this
list updated whenever a new failure mode is found:

| Date | Change                                                                              | Result                              |
|------|-------------------------------------------------------------------------------------|-------------------------------------|
| 2026-05-15 | `fragment@1`: add `rockchip,clk-trcm = <1>` AND `/delete-property/ rockchip,trcm-sync-tx-only` on `i2s@fe480000` | Boot hangs in userspace, no network |
| 2026-05-15 | `fragment@1`: add `rockchip,clk-trcm = <1>` alone on `i2s@fe480000`                  | Boot hangs in userspace, no network |
| 2026-05-15 | `fragment@0`: add `sdmode-gpios` + `gainsel_{1,2,3}-gpios` + `default-volume` to `audio-codec-0`, **drop `pinctrl-0` from fragment@1** | Boot hangs (Tailscale offline; under investigation — likely the dropped pinctrl-0, since GPIOs alone on audio-codec-0 should be inert) |

### Known-good overlay structure

The minimum overlay that brings up the codec card without breaking boot
is the originally-deployed `.bak`:

- `audio-codec-0` with `compatible = "uctronics,uctronics-codec"`, no
  GPIO properties, no `default-volume`.
- `uctronics-sound` with the multicodecs binding to `&i2s1_8ch`.
- `fragment@1` on `&i2s1_8ch`: `status = "okay"`,
  `rockchip,playback-channels = <2>`, `rockchip,capture-channels = <2>`,
  **and an explicit `pinctrl-0 = <&i2s1m0_lrck &i2s1m0_sclk &i2s1m0_sdi0 &i2s1m0_sdo0>;`**.

Mic capture works at S32_LE 48 kHz with this overlay (the volume mixer
is a no-op since the GPIOs aren't wired, but the speaker is still
audible at hardware-default gain).

### When in doubt

- Persistent journald is enabled (`Storage=persistent`); after recovery,
  `sudo journalctl -b -1` shows the previous (failed) boot's logs.
- If a future overlay has the GPIOs hooked up and still breaks boot,
  inspect `journalctl -b -1 -p err` for the unit/probe that stalled
  before deciding which fragment is to blame.

## Piper TTS fails with `symbol lookup error`

```
/usr/local/bin/piper: symbol lookup error:
/usr/local/lib/libpiper_phonemize.so.1:
undefined symbol: espeak_TextToPhonemesWithTerminator
```

The locally-installed `libpiper_phonemize.so.1` references an espeak-ng
symbol that the Armbian-packaged `espeak-ng` (1.51) does not export.
The Piper binary loads the ONNX model (after we hex-patched its IR
version from 9 → 8) but dies before generating audio.

Workaround: use `espeak-ng -w` directly for cue tones; defer Piper
re-install / `piper-rknn-rs` port. See `docs/TODO.md`.
