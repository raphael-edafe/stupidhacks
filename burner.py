#!/usr/bin/env python3
"""
Legend of the Desktop 3D - a 3D Zelda-like game that IS your app launcher.

Setup:   pip install ursina
Run:     python3 legend_of_the_desktop_3d.py

Controls:
  WASD / arrows  move
  SPACE          swing sword
  E              open chest (launches the app inside)
  F11            toggle fullscreen
  ESC            quit

Gags included:
  - Opening a chest shows "You got FIREFOX!" and spins the chest
  - The Terminal chest is locked until you beat the boss
  - Dying closes every app you opened ("your windows died with you")
"""
import math
import random
import shutil
import subprocess

from ursina import *

# Map legend:
#   #  wall          .  floor        P  player start
#   e  enemy         B  boss (guards the terminal)
#   1 2 3  app chests                T  terminal chest (locked until boss dies)
MAP = [
    "####################",
    "#P.....#.....1.....#",
    "#......#...........#",
    "#..e...#....e......#",
    "#......###.####....#",
    "#..................#",
    "#...2.......e......#",
    "#..........####.####",
    "####.#######.......#",
    "#..........#.......#",
    "#..e.......#...B...#",
    "#......3...#.......#",
    "#..........#...T...#",
    "#..........#.......#",
    "####################",
]

# Which app each chest launches: (display name, command)
APPS = {
    "1": ("Firefox", ["firefox"]),
    "2": ("Files", ["nautilus"]),
    "3": ("Text Editor", ["gnome-text-editor"]),
    "T": ("Terminal", ["gnome-terminal"]),
}

W, H = len(MAP[0]), len(MAP)

# Colors
FLOOR = color.hex("#3f7d4a")
WALL = color.hex("#8a8aa0")
CHEST = color.hex("#8c5a28")
LOCKED = color.hex("#463228")
GOLD = color.hex("#f0c83c")
PLAYER = color.hex("#3cc85a")
ENEMY = color.hex("#c83232")
BOSS = color.hex("#963cc8")
HEART = color.hex("#dc283c")
HEART_EMPTY = color.hex("#46282d")

app = Ursina(title="Legend of the Desktop 3D", development_mode=False)
window.color = color.hex("#1a2230")
window.exit_button.visible = False
window.fps_counter.enabled = True


# ---------- grid helpers ----------
# Map column -> world x, map row -> world z (rows go "away" from the camera)
def to_world(col, row):
    return Vec3(col, 0, H - 1 - row)


def to_tile(x, z):
    return round(x), H - 1 - round(z)


solid_tiles = set()  # walls + chests, used for collision


def blocked(x, z, r):
    for cx, cz in ((x - r, z - r), (x + r, z - r), (x - r, z + r), (x + r, z + r)):
        if to_tile(cx, cz) in solid_tiles:
            return True
    return False


def move(ent, dx, dz, r):
    # Move one axis at a time so things slide along walls.
    if dx and not blocked(ent.x + dx, ent.z, r):
        ent.x += dx
    if dz and not blocked(ent.x, ent.z + dz, r):
        ent.z += dz


# ---------- static world ----------
Entity(model="plane", scale=(W, 1, H), position=(W / 2 - 0.5, 0, H / 2 - 0.5),
       color=FLOOR, texture="white_cube", texture_scale=(W, H))

for row, line in enumerate(MAP):
    for col, ch in enumerate(line):
        if ch == "#":
            solid_tiles.add((col, row))
            Entity(model="cube", position=to_world(col, row) + Vec3(0, 0.75, 0),
                   scale=(1, 1.5, 1), color=WALL, texture="brick")

DirectionalLight(y=10, rotation=(45, -45, 0))
AmbientLight(color=color.rgba(0.6, 0.6, 0.6, 1))


# ---------- actors ----------
class Enemy(Entity):
    def __init__(self, pos, boss=False):
        size = 1.2 if boss else 0.6
        super().__init__(model="cube", position=pos + Vec3(0, size / 2, 0),
                         scale=size, color=BOSS if boss else ENEMY)
        self.boss = boss
        self.base_color = self.color
        self.hp = self.max_hp = 8 if boss else 2
        self.radius = 0.45 if boss else 0.28
        self.speed = 1.8 if boss else 1.5
        self.hurt = 0.0
        self.dir = Vec3(0, 0, 0)
        self.timer = 0.0
        if boss:
            self.bar_bg = Entity(parent=self, model="quad", color=HEART_EMPTY,
                                 scale=(1, 0.08), y=0.75, billboard=True)
            self.bar = Entity(parent=self.bar_bg, model="quad", color=HEART,
                              origin=(-0.5, 0), x=-0.5, z=-0.01)

    def tick(self, dt):
        if self.hurt > 0:
            self.hurt -= dt
            self.color = color.white if int(self.hurt * 20) % 2 else self.base_color
        else:
            self.color = self.base_color
        if self.boss:
            # The boss lumbers toward you when you get close.
            to_p = player.position - self.position
            to_p.y = 0
            if to_p.length() < 6:
                d = to_p.normalized() * self.speed * dt
                move(self, d.x, d.z, self.radius)
            self.bar.scale_x = self.hp / self.max_hp
        else:
            # Regular enemies wander randomly.
            self.timer -= dt
            if self.timer <= 0:
                self.dir = random.choice([Vec3(1, 0, 0), Vec3(-1, 0, 0),
                                          Vec3(0, 0, 1), Vec3(0, 0, -1), Vec3(0, 0, 0)])
                self.timer = random.uniform(0.5, 1.5)
            move(self, self.dir.x * self.speed * dt, self.dir.z * self.speed * dt, self.radius)
            self.rotation_y += 90 * dt  # little idle spin so they feel alive


player = Entity(model="cube", color=PLAYER, scale=(0.6, 0.8, 0.6))
Entity(parent=player, model="cube", color=color.white, scale=(0.3, 0.2, 0.3), position=(0, 0.2, 0.5))  # "face"
sword_pivot = Entity(parent=player, y=0)
sword = Entity(parent=sword_pivot, model="cube", color=color.hex("#f0e678"),
               scale=(0.12 / 0.6, 0.12 / 0.8, 1.2 / 0.6), position=(0, 0, 1.0), enabled=False)

state = {"hp": 6, "max_hp": 6, "hurt": 0.0, "swing": 0.0, "rupees": 0, "msg_timer": 0.0}
procs = []      # apps we launched, so dying can close them
enemies = []
chests = []     # list of dicts with entity + metadata


def build_level():
    for e in enemies:
        destroy(e)
    enemies.clear()
    for c in chests:
        destroy(c["ent"])
        destroy(c["label"])
        solid_tiles.discard(c["tile"])
    chests.clear()

    for row, line in enumerate(MAP):
        for col, ch in enumerate(line):
            pos = to_world(col, row)
            if ch == "P":
                player.position = pos + Vec3(0, 0.4, 0)
            elif ch == "e":
                enemies.append(Enemy(pos))
            elif ch == "B":
                enemies.append(Enemy(pos, boss=True))
            elif ch in APPS:
                locked = ch == "T"
                ent = Entity(model="cube", position=pos + Vec3(0, 0.3, 0),
                             scale=(0.8, 0.6, 0.6), color=LOCKED if locked else CHEST)
                Entity(parent=ent, model="cube", color=GOLD, scale=(1.02, 0.12, 1.02), y=0.2)
                label = Text(APPS[ch][0], parent=scene, position=pos + Vec3(0, 1.1, 0),
                             billboard=True, origin=(0, 0), scale=8, color=color.white)
                solid_tiles.add((col, row))
                chests.append({"ent": ent, "label": label, "key": ch,
                               "locked": locked, "tile": (col, row), "cooldown": 0.0})
    player.rotation_y = 180
    state["hp"] = state["max_hp"]
    state["hurt"] = 0.0


# ---------- HUD ----------
hearts = [Entity(parent=camera.ui, model="quad", color=HEART, scale=0.035,
                 position=(-0.85 + i * 0.045, 0.45)) for i in range(state["max_hp"])]
rupee_text = Text("Rupees: 0", parent=camera.ui, position=(0.62, 0.47), color=GOLD, scale=1.3)
message = Text("", parent=camera.ui, origin=(0, 0), y=-0.4, scale=1.4,
               background=True, color=color.white)


def say(text, seconds=3.0):
    message.text = text
    message.create_background()
    message.enabled = True
    state["msg_timer"] = seconds


# ---------- actions ----------
def launch(name, cmd):
    if not shutil.which(cmd[0]):
        say(f"The chest is empty... ({cmd[0]} isn't installed)")
        return
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    procs.append(p)
    say(f"You got {name.upper()}!")


def interact():
    for c in chests:
        if distance_xz(player, c["ent"]) < 1.2:
            if c["locked"]:
                say("It's locked. Something big is guarding it...")
            elif c["cooldown"] <= 0:
                c["cooldown"] = 1.0
                c["ent"].animate_rotation_y(c["ent"].rotation_y + 360, duration=0.6)
                name, cmd = APPS[c["key"]]
                launch(name, cmd)
            return


def swing():
    if state["swing"] > 0:
        return
    state["swing"] = 0.25
    sword.enabled = True
    sword_pivot.rotation_y = -70
    sword_pivot.animate_rotation_y(70, duration=0.2, curve=curve.linear)


def die():
    closed = 0
    for p in procs:
        if p.poll() is None:
            p.terminate()
            closed += 1
    procs.clear()
    build_level()  # the dungeon resets, boss and all
    say(f"You died. {closed} of your windows died with you.", 4.0)


def input(key):
    if key == "space":
        swing()
    elif key == "e":
        interact()
    elif key == "f11":
        window.fullscreen = not window.fullscreen
    elif key == "escape":
        application.quit()


# ---------- per-frame logic ----------
def update():
    dt = min(time.dt, 0.05)

    # Movement (camera looks "north", so W moves away from it)
    dx = (held_keys["d"] or held_keys["right arrow"]) - (held_keys["a"] or held_keys["left arrow"])
    dz = (held_keys["w"] or held_keys["up arrow"]) - (held_keys["s"] or held_keys["down arrow"])
    if dx or dz:
        d = Vec3(dx, 0, dz).normalized()
        player.rotation_y = math.degrees(math.atan2(d.x, d.z))
        move(player, d.x * 4 * dt, d.z * 4 * dt, 0.28)

    # Timers
    if state["hurt"] > 0:
        state["hurt"] -= dt
        player.visible = int(state["hurt"] * 12) % 2 == 0
    else:
        player.visible = True
    if state["swing"] > 0:
        state["swing"] -= dt
        if state["swing"] <= 0:
            sword.enabled = False
    if state["msg_timer"] > 0:
        state["msg_timer"] -= dt
        if state["msg_timer"] <= 0:
            message.enabled = False
    for c in chests:
        if c["cooldown"] > 0:
            c["cooldown"] -= dt

    fwd = player.forward
    fwd.y = 0
    for e in enemies[:]:
        e.tick(dt)
        to_e = e.position - player.position
        to_e.y = 0
        dist = to_e.length()

        # Sword hits: enemy must be close and roughly in front of you
        reach = 1.8 if e.boss else 1.4
        if state["swing"] > 0 and e.hurt <= 0 and dist < reach and \
                (dist < 0.01 or to_e.normalized().dot(fwd) > 0.3):
            e.hp -= 1
            e.hurt = 0.35
            kb = to_e.normalized() * 0.6
            move(e, kb.x, kb.z, e.radius)
            if e.hp <= 0:
                enemies.remove(e)
                state["rupees"] += 5 if e.boss else 1
                if e.boss:
                    for c in chests:
                        c["locked"] = False
                        c["ent"].color = CHEST
                    say("The Terminal Guardian falls! The terminal is yours.", 4.0)
                destroy(e)
                continue

        # Enemy touches player
        touch = 0.9 if e.boss else 0.6
        if state["hurt"] <= 0 and dist < touch:
            state["hp"] -= 2 if e.boss else 1
            state["hurt"] = 1.0
            kb = (-to_e).normalized() * 0.8 if dist > 0.01 else Vec3(0, 0, -0.8)
            move(player, kb.x, kb.z, 0.28)
            if state["hp"] <= 0:
                die()
                return

    # HUD
    for i, h in enumerate(hearts):
        h.color = HEART if i < state["hp"] else HEART_EMPTY
    rupee_text.text = f"Rupees: {state['rupees']}"

    # Zelda-style camera: above and behind, always looking at the player
    target = player.position + Vec3(0, 9, -6.5)
    camera.position = lerp(camera.position, target, min(1, dt * 6))
    camera.look_at(player)


def distance_xz(a, b):
    return math.hypot(a.x - b.x, a.z - b.z)


build_level()
camera.position = player.position + Vec3(0, 9, -6.5)
camera.look_at(player)
say("It's dangerous to go alone! Press E to open a chest.", 5.0)

if __name__ == "__main__":
    app.run()
