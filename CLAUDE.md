# CLAUDE.md — Rules for Claude Code in this repo

## CRITICAL: Do NOT build on the X61s

The X61s (Core 2 Duo L7500) is the dev/edit machine ONLY. It is too slow
and the wrong architecture (x86_64 vs aarch64).

**NEVER run on the X61s:**
- `cargo build`
- `cargo check`
- `cargo clippy`
- `cargo test`
- `cargo doc`
- Any cargo command that compiles or type-checks

**Allowed on the X61s:**
- `cargo fmt` (formatting only)
- Editing code
- Git operations
- Running `scripts/rock-*.sh` scripts to sync/build/deploy to the Rock

**All builds happen on the Rock 5A via:**
```bash
scripts/rock-sync.sh   # sync code
scripts/rock-build.sh  # build + test on Rock
scripts/rock-run.sh    # launch TUI on Rock display
```

## Build workflow

1. Edit code on X61s
2. `scripts/rock-sync.sh` to sync to Rock
3. `cargo check` and `cargo clippy` on Rock via SSH (or `scripts/rock-build.sh`)
4. `scripts/rock-build.sh` to build and test on Rock
5. `scripts/rock-run.sh` to launch on Rock display
6. `scripts/rock-log.sh` to read output log

## Pre-commit hooks

Hooks run `rustfmt` and `clippy` automatically on commit. Install:
```bash
cp scripts/pre-commit .git/hooks/pre-commit
```

## Rock access

Rock IP, user, and password are in `config.json` (single source of truth).
All `scripts/rock-*.sh` scripts read from it automatically.

```bash
scripts/rock-ssh.sh          # interactive SSH
scripts/rock-ssh.sh "cmd"    # run a command
```

To change the Rock IP, edit `config.json` — no other files need updating.
