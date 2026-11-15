// Desktop Siege: doors. All original.
// Light from the top-left. Same warm plank ramp as tiles_plank and the iron of the ore tiles.
//
// door_closed: 16x48 (3 tiles), fully opaque: iron frame, three vertical planks, two hinges on
//              the left, brass handle on the right just below the middle.
// door_open:   16x48, the door swung open and seen edge-on against the hinge (left) side,
//              a faint frame line along the top, everything else transparent.
// door_icon:   16x16 inventory icon of the closed door.

pal({
  k: "#181424",
  // planks (same as terrain)
  t: "#2a1612", u: "#56321f", v: "#865334", w: "#b07646", x: "#d49e64",
  // iron (dark -> light)
  I: "#2a2636", J: "#3c3648", K: "#7c7590", L: "#a8a1b8",
  // brass handle
  G: "#a8641c", g: "#f0a830", y: "#ffe07a",
  // faint door-frame line
  f: "#18142488",
})

// Plank columns: lit left edge, face, shaded right edge, dark seam.
const PLANK = ["x", "w", "w", "v", "u"]

// Iron hinge strap (pointing right from the left frame), 3 rows tall.
function hinge(c, y, len) {
  c.rect(1, y, len, 3, "J")
  c.hline(1, y, len, "K")
  c.hline(1, y + 2, len, "I")
  c.px(len, y + 1, "J"); c.px(len + 1, y + 1, "I") // tapered tip
  c.px(len, y, "J"); c.px(len, y + 2, "I")
  // knuckle on the frame edge
  c.vline(1, y - 1, 5, "J"); c.px(1, y - 1, "K")
  c.px(1, y + 3, "I")
  // rivets
  c.px(4, y + 1, "L"); c.px(len - 2, y + 1, "L")
}

sprite("door_closed", { w: 16, h: 48 }, (c) => {
  // planks inside the iron frame: x 2..13
  for (let x = 2; x <= 13; x++) {
    const k = PLANK[(x - 2) % 4 === 3 && x !== 13 ? 4 : (x - 2) % 4]
    c.vline(x, 2, 44, k)
  }
  // seams darkest where they meet the frame top
  c.vline(13, 2, 44, "v")
  // grain streaks, one or two per plank
  c.vline(3, 12, 4, "v"); c.vline(7, 31, 5, "v"); c.vline(11, 17, 4, "v"); c.vline(3, 36, 3, "v")
  c.px(8, 6, "t"); c.px(12, 40, "t") // knots
  // shadow under the top frame
  c.hline(2, 2, 12, "v"); c.px(5, 2, "u"); c.px(9, 2, "u")
  // iron frame: lit top/left, dark bottom/right
  c.rect(1, 1, 14, 1, "L")
  c.vline(1, 1, 46, "K")
  c.vline(14, 1, 46, "J")
  c.hline(1, 46, 14, "J")
  c.px(1, 1, "L"); c.px(14, 1, "K"); c.px(1, 46, "J")
  // middle iron cross-band with rivets
  c.hline(2, 22, 12, "K"); c.hline(2, 23, 12, "J")
  for (const rx of [3, 7, 11]) c.px(rx, 22, "L")
  // hinges on the left
  hinge(c, 7, 7)
  hinge(c, 37, 7)
  // handle at ~60% height: back plate + brass ring
  c.rect(11, 26, 2, 5, "J"); c.vline(11, 26, 5, "K")
  c.px(12, 28, "g"); c.px(11, 28, "y"); c.px(11, 29, "G"); c.px(12, 29, "G")
  c.px(13, 28, "G")
  // outline
  c.box(0, 0, 16, 48, "k")
})

// Open door, edge-on: a 5px slab on the hinge side (outline included).
sprite("door_open", { w: 16, h: 48 }, (c) => {
  c.hline(0, 0, 16, "f")
  c.rect(0, 0, 5, 48, "k")
  c.vline(1, 1, 46, "x")
  c.vline(2, 1, 46, "w")
  c.vline(3, 1, 46, "v")
  // iron bands at top, middle, bottom
  for (const [y, h] of [[1, 1], [22, 2], [46, 1]]) { c.rect(1, y, 3, h, "J"); c.hline(1, y, 3, "K") }
  // hinges: knuckles on the left edge
  for (const y of [6, 36]) {
    c.rect(1, y, 2, 5, "J"); c.vline(1, y, 5, "K"); c.px(1, y + 2, "L"); c.px(2, y + 4, "I")
  }
  // handle sticking out of the door face
  c.px(4, 28, "J"); c.px(5, 28, "y"); c.px(6, 28, "g")
  c.px(5, 27, "k"); c.px(6, 27, "k"); c.px(7, 28, "k"); c.px(5, 29, "k"); c.px(6, 29, "k")
})

// Inventory icon: drawn 14x14 and grown by the outline to 16x16.
sprite("door_icon", { w: 14, h: 14, outline: "k" }, (c) => {
  const x0 = 2, w = 10
  c.rect(x0, 0, w, 14, "J")
  c.hline(x0, 0, w, "L"); c.vline(x0, 0, 14, "K")
  // planks x 3..10
  const cols = ["x", "w", "v", "u", "x", "w", "w", "v"]
  cols.forEach((k, i) => c.vline(x0 + 1 + i, 1, 12, k))
  c.hline(x0 + 1, 1, 8, "v")
  // cross band
  c.hline(x0 + 1, 6, 8, "J"); c.px(x0 + 2, 6, "K"); c.px(x0 + 7, 6, "K")
  // hinges
  for (const y of [2, 10]) {
    c.hline(x0, y, 4, "K"); c.hline(x0, y + 1, 4, "I"); c.px(x0 + 4, y + 1, "I")
  }
  // handle
  c.vline(x0 + 8, 7, 4, "J"); c.px(x0 + 7, 8, "y"); c.px(x0 + 8, 8, "g"); c.px(x0 + 7, 9, "G")
})
