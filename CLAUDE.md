# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Active plan: read DEMO_PLAN.md first

Two people, each with their own Claude Code session, are building **"Desktop Siege"** (working name) in about 4 hours, for a 60-second demo video. It is a nested Smithay Wayland compositor where the desktop is a Terraria-like 2D world. You build screens out of blocks, real app windows render inside those screens, and at night mobs attack them. **A destroyed screen really closes its app.** That deliberately overrides the "never close apps" rule in `PROJECT_SPEC.md`, but it only affects apps running inside the nested compositor. `--no-kill` disables it.

`PROJECT_SPEC.md` describes the long-term product (DRM backend, ISO, .deb, VirtualBox). It is **out of scope** for the demo. Don't build any of it unless asked. `burner.py` is an unrelated Python/Ursina prototype; leave it alone.

Priorities for the demo: it must look good and run smoothly at 60 FPS (on a VirtualBox VM). Visual polish beats features. Anything that won't appear on camera can wait.

Art must be original or CC0. Terraria-like gameplay is fine, but no Terraria assets, name or look.

## Ownership (avoid merge conflicts)

Only edit files you own.

- **B1 (this session: game core, world rendering, compositor):** everything in `game/` and `compositor/` except the two B2 files below. That includes `sky.rs` and `light.rs` (taken over from B2 in round 3), sprites/world/render, screens/ui, player, and the preview.
- **B2 (friend: mobs and HUD):** `game/src/mobs.rs`, its submodules in `game/src/mobs/` (bats, passive animals, their art) and `game/src/hud.rs`. Draw using `c.w`/`c.h`, because the view size is dynamic: 640x360 close or 960x540 far (`GameApi::view_size()`, `Camera::vw/vh`). The mob sprites are ready: `sprites.mob_crawler` (4 frames, 20x14, faces right) and `sprites.mob_bat` (3 frames, 18x12).
- **Compositor (`compositor/`), B1:** Smithay (based on the `smallvil` example), winit nested backend, window ↔ screen binding, input routing, launching and closing apps.
- **`shared/` is the contract** (`shared/src/lib.rs`: `GameApi`, `InputState`, `ScreenInfo`, `GameEvent`). Both people must agree before changing it. Keep it dependency-free.

The game crate has **no Smithay dependency**. Iterate in the standalone `preview` binary. Commit small and often, and pull before pushing.

Interfaces B2 builds on:
- **Mobs** get a `MobCtx` (world, screens, player, particles, camera, sky, events). They attack screens through `screens.targets()` and `screens.damage(id, amount, impact_point, fx, events)`, hit the player with `player.hurt()`, and take sword hits from `player.attack_box()`.
- **Particle helpers:** `fx.debris`, `fx.sparkle`, `fx.explosion`.
- **Drawing helpers** in `gfx::Canvas`: `blend`, `glow` (additive), `sprite_shaded`, `sprite_flat`, `text_shadow`, `text_centered`. Sprites can be built from ASCII art with `Sprite::from_ascii`, as in `sprites.rs`.
- **Demo knobs:** `N` skips to dusk, `G` toggles god mode, `F1` hides the HUD, `B` enters build mode (then drag), `T` places a torch.

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
cargo run -p game --bin preview --release   # game alone in a minifb window (--scale 3 for 1080p)
cargo run -p game --bin preview --release -- --shots /tmp/shots   # headless scripted demo -> PNGs + perf line
cargo run -p compositor --release           # nested compositor inside the GNOME session
cargo run -p compositor --release -- --no-kill
cargo test -p game                          # all game tests
cargo test -p game <name>                   # a single test
```

The Rust toolchain installs through rustup to `~/.cargo`. If `cargo` isn't on PATH, run `source ~/.cargo/env`. The compositor's system deps are listed in DEMO_PLAN.md under "Setup commands".
