# Plan: "Desktop Siege": a 4-hour demo build

## Context
You and a friend each have a Claude Code session and about 4 hours to make a polished 60-second demo video. The concept is a Linux desktop that is a Terraria-like 2D world. You **build screens out of blocks**, real app windows **render inside those screens in the world**, and at **night mobs attack the screens**. When a screen is destroyed, its **app really closes**.

`PROJECT_SPEC.md` describes a multi-week product: a DRM backend, a custom ISO, a .deb, VirtualBox handling and about 20 protocols. It also forbids mobs from touching apps. For this demo we **override the spec**. We keep only its useful shape (a Rust/Smithay compositor plus a game crate with no Smithay dependency) and drop everything that won't appear in the video.

"Desktop Siege" is a working name, so rename it freely. The art must be original or CC0: Terraria-like *gameplay*, but no Terraria assets, name or look.

Environment facts (checked): Ubuntu 25.10 in VirtualBox (VMSVGA), GNOME Wayland session on tty2, 4 CPUs, 10 GB RAM. **Rust is not installed and no Wayland dev libraries are present.**

## Approach
Write a **nested Smithay compositor** (winit backend) that runs as a window inside GNOME. In the recording, set the nested window fullscreen at 1920x1080.

### Rendering trick (smooth on a VM, looks good)
The game software-renders pixel art into **two small RGBA buffers at 640x360**. They are uploaded once per frame as `MemoryRenderBuffer`s and drawn at 3x scale with **nearest filtering**. That's crisp pixel art with only about 1 MB of upload per layer per frame and almost no GPU work. Draw order:
1. **World layer** (under): sky gradient that blends from day to night, parallax hills, stars, sun/moon, tiles, light map
2. **Client windows**: each one placed in its screen's interior rect (world→pixel transform through the camera), with a jitter offset when the screen is being hit
3. **FX layer** (over, transparent): player, mobs, particles, crack overlays on damaged screens, screen HP bars, HUD (hotbar, clock, night warning)

### Workspace layout (new)
```
Cargo.toml            # workspace; [profile.dev] opt-level=1, deps opt-level=3
shared/               # the contract between the two people: tiny, agreed first
game/                 # pure Rust: world, player, building, mobs, day/night, software renderer
  src/bin/preview.rs  # minifb window: lets the game person iterate without the compositor
compositor/           # smithay "smallvil" example copied in and extended
assets/               # CC0 sprites (e.g. Kenney pixel packs) or procedural art generated in code
DEMO_PLAN.md          # copy of this plan, so the friend's Claude sees it
```

### The shared contract: `shared/src/lib.rs`
This file is the source of truth (it is already committed). In summary: `VIEW_W/VIEW_H/SCALE` constants, `Key` + `InputState`
(bitset, with a `game_focused` flag), `ScreenInfo { id, rect, hp, glitch }`, `GameEvent::{ScreenBuilt, ScreenDamaged,
ScreenDestroyed}`, and the `GameApi` trait (`update`, `draw_world`, `draw_fx`, `screens`, `set_screen_occupied`,
`drain_events`). The game shakes the rect itself, so the window and its frame move together.

## Work split
**Update:** both people started on the game, so `game/` is split into B1 (core: world, player, screens, rendering) and B2 (night: sky, light, mobs, HUD). See the subtask lists below and the ownership section in CLAUDE.md. The compositor (person A's list) belongs to whoever finishes first.

### B2 subtasks (night and atmosphere): `sky.rs`, `light.rs`, `mobs.rs`, `hud.rs`
1. **Mobs** (highest priority, since the video's climax depends on them): Crawler (walks and hops) and Bat (flies and swoops) with original ASCII-art sprites. They spawn at night off-screen, path to `screens.targets()`, and call `screens.damage()` on contact. Sword hits through `player.attack_box()` give knockback, a white hit flash and a death pop. They burn up at dawn. Tune so one night breaks exactly one screen on camera.
2. **Sky:** stars that fade in, a sun/moon arc, two layers of parallax hills, drifting clouds and a warm dusk band.
3. **Light:** a per-tile light map (sky light from above, torches, crystals and screens as sources) with smooth falloff, so caves are dark and torches glow at night. Keep it cheap: recompute only the visible area.
4. **HUD:** pixel hotbar slots with item icons, hearts, a day/night dial, big "NIGHT IS COMING" and "DAWN" banners, and an FPS readout (hidden with F1).

### B1 subtasks (core): done in the first pass, then polish
World gen, autotiled rendering, player physics, mining and placing, screen building and lifecycle (build animation, booting panel, cracks, glitch, explosion), particles, camera shake, and the preview with headless `--shots`. Next: polish from the screenshot review, then pick up the compositor.

### Person A: compositor (`compositor/`)
1. Copy smithay's `smallvil` (from the git tag matching the pinned smithay version) and get `foot` showing up and taking input in the nested window. **Target: 1 hour.**
2. Run the game on a fixed 60 Hz timer in calloop. Render world buffer → windows → FX buffer, with nearest upscale.
3. Window ↔ screen binding. On `ScreenBuilt`, spawn the next app from a demo list (`foot`, `foot -e htop`, `foot -e cmatrix`, `gnome-calculator`) with `WAYLAND_DISPLAY` set. When its toplevel maps, bind it to that screen, send a configure sized to the interior rect (×SCALE), and reposition it every frame from `screen_rects()`.
4. Input routing: when the pointer is over a bound window, pointer and keyboard go to the app. Clicking the world or pressing `Esc` gives focus back to the game. Translate winit input into `InputState`.
5. On `ScreenDamaged`, jitter the window by a few pixels. On `ScreenDestroyed`, play a short glitch (flicker or offset) for about 0.8 s, then `toplevel.send_close()`. Add a `--no-kill` flag that skips the close, as a safety net.

### Person B: game (`game/`)
1. Seeded world gen: 16 px tiles (shown at 48 px). Surface hills, dirt and stone, a few caves and ores. Autotiled edges and per-tile shading noise so it looks polished. Develop in `preview` from the start.
2. Player: A/D movement, Space to jump, AABB tile collision, smooth camera lerp, left click to mine (break particles), right click to place.
3. **Screen building:** press `B` and drag a rectangle. A frame builds block by block with sparkles, then emits `ScreenBuilt`. The interior shows a glowing "booting" panel until a window arrives.
4. Day/night tuned for the demo: about 40 s day, 30 s night, plus an `N` key to skip to dusk. Sky color lerp, stars, a light map (torches and screens glow), and a "Night is coming…" banner.
5. Mobs: a ground crawler (walks and hops toward the nearest screen) and a flying bat (moves straight in). Each attacks the frame, which lowers screen HP and spawns crack overlays. The player's sword swing knocks mobs back and kills them with a pop and particles. At 0 HP: explosion, then `ScreenDestroyed`.
6. Demo knobs: a fixed seed, `G` for god mode, `N` for night, and `F1` to hide the HUD.

### Together
- **0:00–0:30:** install the toolchain, scaffold the workspace, write and commit the `shared` crate, `DEMO_PLAN.md` and the CLAUDE.md update, and push to a shared git remote.
- **3:00–3:30:** integrate and tune balance (a night should break exactly one screen on camera).
- **3:30–4:00:** script and record the video.

## Setup commands (each machine)
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev libudev-dev \
  libinput-dev libgbm-dev libseat-dev libegl-dev libgles-dev libdrm-dev libpixman-1-dev foot htop cmatrix
```
In VirtualBox, turn on **3D acceleration** (VMSVGA). Then check with `glxinfo -B` (from `mesa-utils`) that the renderer isn't llvmpipe. The 640x360 approach should still hit 60 FPS on llvmpipe either way. Always run with `--release` for recording.

## Files to change in the repo
- New: `Cargo.toml`, `shared/`, `game/`, `compositor/`, `DEMO_PLAN.md`
- Update `CLAUDE.md`: point to `DEMO_PLAN.md` as the active plan, record the ownership split (A = `compositor/`, B = `game/`, `shared/` changes need both people's agreement), and note that the demo deliberately overrides the spec's "never close apps" rule.
- Leave `PROJECT_SPEC.md` and `burner.py` untouched.

## Verification
- `cargo run -p game --bin preview --release`: world, player, building, a night cycle and mobs breaking a screen all work standalone.
- `cargo run -p compositor --release`: a nested window opens. Build a screen and an app appears inside it and accepts typing. Press `N` and mobs damage the screen, and the window shakes. On destruction the app closes (check `pgrep foot`). With `--no-kill` the app survives.
- `cargo test -p game`: world gen is deterministic for a seed, and screen HP reaches 0 and emits `ScreenDestroyed` exactly once.
- FPS counter on the HUD (hidden in the recording) holds a steady 60.

## Demo video script (60 s)
Wide shot of the world at dawn → player mines and builds a screen → a terminal boots inside it → build a second screen with htop → camera pans while typing in the terminal → "Night is coming…" → bats and crawlers swarm, sword fight → one screen cracks, shakes, explodes, and its app closes → dawn breaks over the surviving screen.
