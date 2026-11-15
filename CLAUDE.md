# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Active plan: read DEMO_PLAN.md first

Two people, each with their own Claude Code session, are building **"Desktop Siege"** (working name) in about 4 hours, for a 60-second demo video. It is a nested Smithay Wayland compositor where the desktop is a Terraria-like 2D world. You build screens out of blocks, real app windows render inside those screens, and at night mobs attack them. **A destroyed screen really closes its app.** That deliberately overrides the "never close apps" rule in `PROJECT_SPEC.md`, but it only affects apps running inside the nested compositor. `--no-kill` disables it.

`PROJECT_SPEC.md` describes the long-term product (DRM backend, ISO, .deb, VirtualBox). It is **out of scope** for the demo. Don't build any of it unless asked. `burner.py` is an unrelated Python/Ursina prototype; leave it alone.

Priorities for the demo: it must look good and run smoothly at 60 FPS (on a VirtualBox VM). Visual polish beats features. Anything that won't appear on camera can wait.

Art must be original or CC0. Terraria-like gameplay is fine, but no Terraria assets, name or look.

## Ownership (avoid merge conflicts)

- **Person A owns `compositor/`**: Smithay (based on the `smallvil` example), winit nested backend, window ↔ screen binding, input routing, launching and closing apps.
- **Person B owns `game/`**: world, player, building, mobs, day/night, and the software pixel renderer. It has **no Smithay dependency**. Iterate in the standalone `preview` binary.
- **`shared/` is the contract** (`shared/src/lib.rs`: `GameApi`, `InputState`, `ScreenInfo`, `GameEvent`). Both people must agree before changing it. Keep it dependency-free.

Commit small and often, pull before pushing, and only touch your own crate.

## Architecture

The game software-renders two RGBA layers at **640x360 view pixels** (`shared::VIEW_W/VIEW_H`). The compositor uploads them each frame as memory buffers and draws them at 3x (`SCALE`) with nearest filtering:
1. **World layer**: sky, tiles, lighting. Opaque, drawn under the windows.
2. **Client windows**: each placed at its `ScreenInfo.rect` × SCALE, every frame. The game applies camera and shake to that rect.
3. **FX layer**: player, mobs, particles, cracks, HUD. Transparent, drawn over the windows.

The game ticks at a fixed 60 Hz (`GameApi::update`). Events flow one way:
- `ScreenBuilt`: the compositor launches the next demo app into that screen.
- `ScreenDestroyed`: the compositor plays a glitch, then sends `xdg_toplevel.close`.

The compositor reports window attach/detach back through `set_screen_occupied`. While an app has keyboard focus, `InputState.game_focused` is false and the game ignores input.

## Commands

```
cargo run -p game --bin preview --release   # game alone in a minifb window
cargo run -p compositor --release           # nested compositor inside the GNOME session
cargo run -p compositor --release -- --no-kill
cargo test -p game                          # all game tests
cargo test -p game <name>                   # a single test
```

The Rust toolchain installs through rustup to `~/.cargo`. If `cargo` isn't on PATH, run `source ~/.cargo/env`. The compositor's system deps are listed in DEMO_PLAN.md under "Setup commands".
