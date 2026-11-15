# [GAME NAME] — Project Spec

Build [GAME NAME]: a Wayland compositor and window manager for Ubuntu, written in Rust with Smithay, where the desktop is a playable 2D side-scrolling survival/crafting game in the style of Terraria. It must work as a real daily-driver desktop: it launches, displays and manages normal Linux applications (Wayland and X11 apps via XWayland). It must run properly inside VirtualBox, and the project ships a custom Ubuntu ISO with it preinstalled.

## Core concept
The desktop background is a live, procedurally generated 2D world. The player character explores, mines and gathers resources to build and upgrade the "Terminal", a computer core at the centre of the world. By day the player gathers; when night falls, waves of enemies try to destroy the Terminal, and the player defends it with crafted weapons and defenses. Upgrading the Terminal unlocks window-manager features. Real application windows float above the world, managed by the compositor.

## Golden rules (non-negotiable)
1. The game never gets in the way of real work. Apps are always reachable, focusable and usable, whatever is happening in the game.
2. Game events never close, kill, hide or alter real applications or files. Enemies can only damage game objects.
3. Losing the game resets the game world only. Running apps are untouched.
4. The compositor must not crash. No unwrap()/expect() in runtime paths; errors are logged and handled. A crash takes down every app, so robustness beats features.
5. Base window management (open, move, resize, close, focus, Alt+Tab, app launcher) works from the start. Upgrades add extras on top; they never gate the basics.

## Target platform
- Ubuntu 24.04 LTS and newer, x86_64
- Intel, AMD and NVIDIA (proprietary driver with GBM) GPUs, plus VirtualBox 7.x guests (VMSVGA)
- Selectable as a session at the GDM login screen

## Tech stack
- Rust (stable, edition 2021), Smithay (latest), calloop event loop. Use Smithay's example compositor Anvil as the starting point; niri and cosmic-comp are good references.
- Backends:
  - udev/DRM/KMS + libinput + libseat for running as a real session from a TTY/GDM
  - winit backend for running nested inside an existing desktop during development (`--nested`)
- Rendering: Smithay GlesRenderer with fallbacks (see VirtualBox section); the game is drawn with Smithay render elements (texture atlas for tiles/sprites, solid colour elements for effects), composited under the client windows
- XWayland via Smithay's xwayland support for X11 apps
- Game logic: plain Rust modules or a light ECS (hecs or bevy_ecs used standalone, without the full Bevy engine), fixed-timestep update at 60 Hz, decoupled from compositor frame callbacks
- Audio: rodio (optional, can be muted)
- Config: TOML at ~/.config/[game-name]/config.toml, hot-reloaded
- Logging: tracing, to ~/.local/state/[game-name]/log
- Cargo workspace with crates: `compositor` (Smithay, protocols, backends, input, window management), `game` (world, entities, crafting, enemies; no Smithay dependency so it can be unit-tested), `render` (draws game state into render elements), `config`

## Wayland protocols to support
- Core: wl_compositor, wl_subcompositor, wl_shm, wl_seat (keyboard, pointer, touch), wl_output, wl_data_device (clipboard + drag-and-drop), linux-dmabuf
- xdg-shell (toplevels + popups), xdg-decoration (server-side decorations by default), xdg-activation
- wlr-layer-shell (so panels, notification daemons like mako, and launchers like fuzzel/wofi work)
- primary-selection, viewporter, fractional-scale, presentation-time, xdg-output, pointer-constraints + relative-pointer, idle-inhibit, keyboard-shortcuts-inhibit, text-input/input-method, session-lock (for swaylock), wlr-screencopy + xdg-desktop-portal-wlr compatibility for screenshots and screen sharing
- HiDPI and per-output scaling; multi-monitor hotplug

## Input model: game vs apps
- Desktop mode (default): keyboard and mouse go to the focused app window. Clicking empty world space focuses the world.
- When the world has focus, game controls are active: A/D move, Space jump, left click mine/attack, right click place/use, 1–9 hotbar, E inventory.
- Super+G toggles "Play mode": windows dim and slide aside, and all input goes to the game. Super+G again restores every window exactly as it was.
- Super + key shortcuts always go to the compositor and are never sent to the game or to apps.
- Super+P pauses the game (enemies freeze, the clock stops). Automatically pause when a fullscreen app is focused, on screen lock, and optionally whenever any app has focus ("focus mode" setting).
- Every Super shortcut has a configurable alternative modifier (default Alt+Ctrl when running in a VM; see VirtualBox section).

## Window management
- Floating windows with themed server-side decorations: a pixel-art title bar styled as a stone/metal frame, with close, maximize and minimize buttons. Client-side decorations are respected when an app insists.
- Move (Super+drag or drag the title bar), resize (Super+right-drag or drag the edges), maximize, fullscreen, minimize to the dock
- Focus follows click; Alt+Tab / Super+Tab window switcher
- App launcher (Super+Space or Super+A): reads .desktop files from XDG data dirs (/usr/share/applications, ~/.local/share/applications, Flatpak and Snap export dirs), with search, icons from the icon theme, and launches the app with correct environment variables (WAYLAND_DISPLAY, DISPLAY for XWayland, XDG_CURRENT_DESKTOP)
- Super+Enter opens the user's terminal emulator (configurable; default gnome-terminal, fallback foot)
- Dock/taskbar along the bottom edge, drawn in the game's style: running apps, pinned apps, clock, network/battery/volume indicators
- Workspaces as different areas of the game world (see upgrades); Super+1..9 switches
- Autostart: honour ~/.config/autostart, and start a notification daemon, polkit agent, and xdg-desktop-portal if present

## Terminal upgrades → WM features
The Terminal is a multi-tile structure with a health bar. Each tier costs resources, raises max health, changes its sprite, and unlocks extras:
1. Casing (wood, stone): window snapping to edges/halves
2. Power Supply (copper, iron): crafting station + multiple workspaces (each workspace is a different biome of the world)
3. Motherboard (silicon, circuit scrap): tiling mode (master/stack layout toggle, Super+T) + window switcher live thumbnails
4. Display (crystal, iron): multi-monitor world spanning + an in-world radar overlay; reaching this tier and surviving the next night wins the run

If the Terminal is destroyed, the world resets to tier 0 but unlocked WM features stay unlocked (saved to the player profile), so the user never loses desktop functionality.

## Game world
- 16x16 tile world, procedurally generated from a seed: surface hills, dirt/stone layers, caves, ore veins at depth
- Destructible/placeable tiles; chunked world storage, saved to ~/.local/share/[game-name]/worlds/
- Resources (shallow to deep): wood, stone, copper ore, iron ore, silicon, circuit scrap, crystal
- The camera follows the player in Play mode; in Desktop mode it rests centred on the Terminal (configurable)
- Lighting: darker at night, torches and the Terminal give off light; windows stay fully readable at all times

## Crafting
- Recipes unlocked by Terminal tier; defined in a data file (RON or TOML) for easy tuning
- Tools (better pickaxes mine deeper), melee weapons, ranged weapons (bow → blaster), ammo, defenses (walls, spike traps, auto-turrets powered by the Terminal), healing items

## Day/night and enemies
- Default: about 10 minutes day and 4 minutes night; option to sync with real local time
- Desktop notification (via the user's notification daemon over D-Bus) before nightfall and when the Terminal is under attack
- Enemies spawn at world edges/underground at night and pathfind to the Terminal: Glitch Crawler (melee), Bit Bat (flying), Breaker (destroys placed blocks), Virus Swarm (later nights). Difficulty scales each night; enemies drop resources and retreat at dawn.
- Auto-turrets defend while the user is working in apps, so a night spent in Desktop mode isn't automatically lost (balanced so a tier-2+ base survives an unattended night)
- Optional "calm mode" setting: no enemies; the game is just a building sandbox desktop

## Performance
- Damage tracking: only redraw changed regions; the world at rest must not burn GPU
- Game simulation at 60 Hz in Play mode, throttled (e.g. 10 Hz, or paused) while apps have focus
- Direct scanout for fullscreen apps where possible (never required)
- Target under 2% CPU idle on the desktop with the world visible (on real hardware)

## Ubuntu integration and packaging
- Install a session entry at /usr/share/wayland-sessions/[game-name].desktop so it appears in GDM's session picker
- Launch script sets XDG_CURRENT_DESKTOP=[GameName], XDG_SESSION_TYPE=wayland, starts a D-Bus session if needed, and imports the environment into systemd --user
- Build a .deb (cargo-deb) that depends on xwayland, libseat1, libinput10, libgbm1, libxkbcommon0 and installs the binary, session file, default config and assets
- Keyboard layout taken from the system (localectl / XKB settings) and overridable in config
- Document the apt build dependencies: build-essential, pkg-config, libudev-dev, libinput-dev, libgbm-dev, libxkbcommon-dev, libegl-dev, libgles-dev, libwayland-dev, libseat-dev, libdrm-dev, libpixman-1-dev, xwayland

## VirtualBox compatibility (required)
The compositor must run properly inside a VirtualBox 7.x guest running Ubuntu 24.04 LTS, both installed from the official ISO and booted from our custom ISO.

Graphics:
- The VirtualBox VMSVGA adapter uses the vmwgfx DRM driver. The udev/DRM backend must work on it via Mesa's SVGA driver when "Enable 3D Acceleration" is on.
- Renderer fallback chain, chosen automatically at startup and logged:
  1. GlesRenderer on hardware/virtual GPU (vmwgfx + Mesa SVGA)
  2. GlesRenderer on Mesa llvmpipe (software GL) when 3D acceleration is off
  3. Smithay PixmanRenderer (pure CPU) as a last resort
  Allow forcing one with `--renderer=gles|pixman` or the config file.
- The game must stay playable at 30+ FPS on llvmpipe at 1920x1080: keep draw calls low (one tile texture atlas, batched sprites), keep damage tracking strict, and add a "low graphics" setting (no dynamic lighting, fewer particles) that switches on automatically on software rendering.
- Software cursor fallback: detect vmwgfx/virtual GPUs and default to a composited cursor (config override available).
- Do not depend on direct scanout, overlay planes, VRR or explicit sync; use them only when available.

Display resizing:
- When the VirtualBox window is resized (Guest Additions, "Auto-resize guest display"), vmwgfx announces a new preferred mode through a connector hotplug event. Listen for udev DRM change events, re-read connectors, and switch the output to the new preferred mode without restarting. Re-layout windows, the dock and the game camera.
- Support multiple virtual monitors.

Input:
- VirtualBox's mouse integration uses an absolute pointing device (USB tablet). Handle libinput absolute pointer motion events correctly, mapped to the output layout, alongside normal relative mice.
- With mouse integration on there is no pointer capture, so pointer-constraints/relative-pointer must degrade gracefully.
- The Host key (Right Ctrl by default) is handled by VirtualBox itself. Don't use Right Ctrl in default bindings.
- Many hosts grab Super before the VM sees it. Detect a VM via `systemd-detect-virt` or /sys/class/dmi/id/product_name ("VirtualBox") and default the compositor modifier to Alt+Ctrl there.

Guest Additions:
- Work both with and without Guest Additions installed.
- Start `VBoxClient --vmsvga` and `VBoxClient --clipboard` from the session if they're present. Shared clipboard on Wayland is limited in VirtualBox, so a failure must not break anything; ensure clipboard works through XWayland at minimum. Document the limitation.

Recommended VM settings (document in the README and apply them in `scripts/create-vm.sh`, which uses VBoxManage to create the VM, attach the ISO and start it):
- Type Linux / Ubuntu (64-bit), 4+ CPUs, 8 GB RAM, 40 GB disk
- Graphics Controller: VMSVGA, Video Memory 128 MB, Enable 3D Acceleration: on
- EFI on (the ISO must boot in both EFI and legacy BIOS mode)
- Pointing device: USB Tablet

## Custom Ubuntu ISO
Provide `scripts/build-iso.sh`, which turns the official Ubuntu 24.04 LTS Desktop ISO into a remastered live/installer ISO with [GAME NAME] preinstalled:
- Input: path to the official Ubuntu ISO (download URL and SHA256 documented; the script verifies the checksum). Output: `dist/[game-name]-ubuntu-24.04-amd64.iso`.
- Tools: xorriso, squashfs-tools, and a chroot (or systemd-nspawn), run on an Ubuntu host. Handle Ubuntu 24.04's layered squashfs layout (minimal / standard / live layers): add our packages to the right layer, and update the manifest and filesystem.size files.
- In the chroot: install our .deb plus dependencies, xwayland, virtualbox-guest-utils, foot as a fallback terminal, and mako
- Set GDM's default session to [GAME NAME] for the live user and for newly installed users (AccountsService config); keep stock GNOME available as a fallback session
- Optional autologin straight into the game session on the live ISO (`--live-autologin`)
- Keep the normal Ubuntu installer working, so installing from the ISO gives a system with our session already set up
- Rebuild GRUB configs for EFI and BIOS and produce a hybrid ISO that boots in VirtualBox and from USB
- The script is idempotent, cleans up mounts on failure (trap), and uses sudo only where required

## Emergency and safety
- Ctrl+Alt+Backspace (configurable) exits the compositor cleanly
- Ctrl+Alt+F1–F12 VT switching works
- `--safe` flag starts with the game disabled: plain WM only
- If the game module panics, catch it at the boundary, disable the game, and keep the compositor and apps running

## Testing
- Unit tests for the `game` crate (world gen, crafting, combat, day/night)
- `scripts/vm-smoke-test.sh`: boots the ISO headless in VirtualBox (VBoxManage startvm --type headless), waits for the session, takes a screenshot with VBoxManage controlvm screenshotpng, and checks the compositor log (shared folder or serial console) for successful startup
- Test matrix: 3D acceleration on / off (llvmpipe), Guest Additions on / off, window resize while running, 2 virtual monitors, EFI and BIOS boot

## Deliverable and build order
- `cargo run -- --nested` for development, `cargo deb` for the package, `scripts/build-iso.sh` for the ISO, `scripts/create-vm.sh` for the VirtualBox VM
- README: build deps, running nested, building the ISO, creating the VM, VirtualBox settings and known limitations, keybindings, config reference, balance data
- Build in this order and keep each stage working before moving on:
  1. Minimal compositor (winit backend): show and focus xdg-shell windows, move/resize, launch a terminal
  2. udev/DRM backend with libseat; test in VirtualBox early (VMSVGA with 3D on and off), including the renderer fallback chain and the software cursor
  3. Hotplug/resize handling, absolute pointer input, VM keybinding fallback, XWayland
  4. Launcher, dock, Alt+Tab, layer-shell, clipboard, required protocols
  5. .deb packaging, then build-iso.sh and create-vm.sh; boot the custom ISO in VirtualBox into the session
  6. Render the generated world as the background; player, mining, inventory, input modes
  7. Terminal building, crafting, day/night, enemies
  8. Upgrade-linked WM features, workspaces as biomes, polish, smoke test
