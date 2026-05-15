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
[13_SKILLS.md](13_SKILLS.md) "Mount the Rock's microSD on the dev machine".

### Confirmed-bad overlay variants (do not redeploy these)

These are the changes that have hung Armbian boot to date — keep this
list updated whenever a new failure mode is found:

| Date | Change                                                                              | Result                              |
|------|-------------------------------------------------------------------------------------|-------------------------------------|
| 2026-05-15 | `fragment@1`: add `rockchip,clk-trcm = <1>` AND `/delete-property/ rockchip,trcm-sync-tx-only` on `i2s@fe480000` | Boot hangs in userspace, no network |
| 2026-05-15 | `fragment@1`: add `rockchip,clk-trcm = <1>` alone on `i2s@fe480000`                  | Boot hangs in userspace, no network |
| 2026-05-15 | `fragment@0`: add `sdmode-gpios` + `gainsel_{1,2,3}-gpios` + `default-volume` to `audio-codec-0`, **drop `pinctrl-0` from fragment@1** | Boot hangs (Tailscale offline). Initially blamed on missing pinctrl-0; pinctrl-0 was restored on the next iteration and boot STILL hung. |
| 2026-05-15 | Same as above but with `pinctrl-0` restored on `fragment@1`. | Boot still hangs. **Root cause turned out to be a misread of the baseline DT**: the GPIO phandle `0x16a` used by the baseline audio-codec-0 belongs to `gpio@fec20000` (= GPIO bank **1**), not `gpio@fec40000` (= bank 3). Our overlay had been declaring `&gpio3` for sdmode/gainsel; on Armbian those GPIO3 pins are claimed/pinmuxed for something else and grabbing them hung systemd. Fix: switch to `&gpio1` (next attempt). |

### Known-good overlay structure (2026-05-15, after GPIO bank fix)

The current `hardware/uctronics-audio/uctronics-audio-overlay.dts`:

- `audio-codec-0` with `compatible = "uctronics,uctronics-codec"`,
  `sdmode-gpios = <&gpio1 13 0>` (GPIO1_B5), `gainsel_{1,2,3}-gpios`
  on `&gpio1 3/5/2`, `default-volume = <2>`. **All on bank 1, not
  bank 3** — see history table above.
- `uctronics-sound` with the multicodecs binding to `&i2s1_8ch`.
- `fragment@1` on `&i2s1_8ch`: `status = "okay"`,
  `rockchip,playback-channels = <2>`, `rockchip,capture-channels = <2>`,
  **and an explicit `pinctrl-0 = <&i2s1m0_lrck &i2s1m0_sclk &i2s1m0_sdi0 &i2s1m0_sdo0>;`**.
- **No** `rockchip,clk-trcm` and **no** `/delete-property/ rockchip,trcm-sync-tx-only`.

Mic capture works at S32_LE 48 kHz. Speaker plays with `sdmode` driven
correctly. The gain GPIOs are claimed (`sdmode=yes gain=2` in
codec dmesg) but **the 5 gain levels produced no perceptible volume
difference** in side-by-side testing; see `docs/09_AUDIO.md` for the
working theories. Use software amplification for now.

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
re-install / `piper-rknn-rs` port. See `docs/14_TODO.md`.
