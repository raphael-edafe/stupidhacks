// Desktop Siege: trees and surface decorations. All original.
// Light from the top-left. Foliage is built from overlapping leaf puffs, each banded from a
// lit top-left to a shaded bottom-right, with a leaf-shaped noise breaking the band edges.
//
// Trees (game: TreeKind order = oak, pine, blossom, autumn):
//   canopy_<kind>  : canopy sprite, drawn over the top log (anchor table in sprites.rs)
//   trunk_<kind>   : 48x16 strip, 3 trunk segment variants (16x16, transparent sides)
//   roots_<kind>   : 32x16 root flare that replaces the bottom log, bottom row = ground line
// Decor: decor.png, 16x16 cells, bottom row = ground line (order = DecorKind in world.rs).

pal({
  k: "#181424",
  // oak foliage: deep -> highlight
  1: "#123024", 2: "#1d4e2c", 3: "#2f7434", 4: "#4f9c3c", 5: "#86c654", 6: "#c0e682",
  // pine foliage (cooler, bluer)
  7: "#0e2626", 8: "#16403a", 9: "#215e4c", a: "#36805c", b: "#62a874",
  // blossom (pink)
  c: "#4a1f3c", d: "#8a3460", e: "#c85a8a", f: "#ee8eb4", g: "#ffc4dc", h: "#fff2f8",
  // autumn (amber/rust)
  i: "#3a1418", j: "#7a2a1c", l: "#b8501e", m: "#e48a2a", n: "#ffc65a", o: "#fff0a8",
  // oak / autumn bark
  p: "#1e1216", q: "#3e2620", r: "#5e3a28", s: "#835434", t: "#a8733f",
  // pine bark (redder)
  u: "#3a1c1a", v: "#6a3426", w: "#94523a",
  // blossom bark (grey-brown)
  x: "#262028", y: "#463a40", z: "#685860", A: "#8e7e84",
  // flowers / mushrooms / pebbles
  B: "#ff5a5a", C: "#a8203a", D: "#ffe066", E: "#c08a1c", F: "#8ab8ff", G: "#3e5ac8", W: "#ffffff",
  H: "#f4ead8", I: "#c4b49c", J: "#8a7462", K: "#5a4a40", L: "#9a98a8", M: "#6a687c", N: "#c8c8d4",
  // grass (same as the terrain grass)
  O: "#15301f", P: "#205428", Q: "#317a34", R: "#4fa240", S: "#86cc58", T: "#c4ec86",
})

function rng(seed) {
  let s = seed >>> 0
  return () => {
    s = (s + 0x6d2b79f5) >>> 0
    let t = s
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}
// Leaf-shaped value noise: small scallops (2-3 px) in [-1, 1].
function leafNoise(x, y, salt) {
  const cx = Math.floor(x / 3), cy = Math.floor(y / 2)
  const h = (a, b) => {
    let v = Math.imul(a * 374761393 + b * 668265263 + salt * 2147483647, 1274126177)
    v ^= v >>> 13; v = Math.imul(v, 1103515245); v ^= v >>> 16
    return ((v >>> 0) % 1000) / 1000
  }
  const ox = (y >> 1) % 2 ? 1.5 : 0 // brick offset so the scallops interlock
  const fx = (x + ox) / 3 - Math.floor((x + ox) / 3), fy = y / 2 - cy
  const bump = 1 - Math.hypot(fx - 0.5, (fy - 0.5) * 1.3) * 1.6
  return (h(Math.floor((x + ox) / 3), cy) * 2 - 1) * 0.5 + bump * 0.6
}

// Paint leaf puffs. ramp = 6 keys deep -> highlight. Puffs: [cx, cy, r, depth 0 back .. 1 front].
function puffs(c, list, ramp, salt) {
  const [k0, k1, k2, k3, k4, k5] = ramp
  for (const [cx, cy, r, depth] of list) {
    for (let y = Math.floor(cy - r - 2); y <= cy + r + 2; y++) for (let x = Math.floor(cx - r - 2); x <= cx + r + 2; x++) {
      if (x < 0 || y < 0 || x >= c.w || y >= c.h) continue
      const nx = (x + 0.5 - cx) / r, ny = (y + 0.5 - cy) / r
      const d = Math.hypot(nx, ny)
      const ln = leafNoise(x, y, salt)
      if (d > 1 + ln * 0.12) continue
      let lam = -(nx * 0.62 + ny * 0.78) + ln * 0.28 + (depth - 0.5) * 0.5
      // the rim facing away from the light gets the darkest band
      if (d > 0.82 && nx + ny > 0.3) lam -= 0.35
      const k = lam > 0.78 && d < 0.9 ? k5 : lam > 0.35 ? k4 : lam > -0.05 ? k3 : lam > -0.45 ? k2 : lam > -0.85 ? k1 : k0
      c.px(x, y, k)
    }
  }
}

// Branch line (2px) in bark colours, used inside canopies.
function branch(c, x0, y0, x1, y1, dark, lit) {
  c.line(x0, y0, x1, y1, dark, 1)
  c.line(x0 - 1, y0, x1 - 1, y1, lit, 1)
}

// ---------------------------------------------------------------- canopies

const OAK = ["1", "2", "3", "4", "5", "6"]
const AUT = ["i", "j", "l", "m", "n", "o"]
const BLO = ["c", "d", "e", "f", "g", "h"]

function roundCanopy(c, ramp, salt, bark) {
  // trunk / branches showing through the lower canopy
  branch(c, 27, 47, 27, 30, bark[0], bark[1])
  branch(c, 27, 38, 18, 30, bark[0], bark[1])
  branch(c, 28, 36, 38, 28, bark[0], bark[1])
  // back row (darker), then the front puffs
  puffs(c, [[14, 22, 10, 0.1], [40, 21, 10, 0.1], [27, 14, 11, 0.2]], ramp, salt)
  puffs(c, [[9, 31, 8, 0.4], [45, 31, 8, 0.35], [20, 30, 10, 0.6], [35, 30, 10, 0.55], [26, 11, 9, 0.8], [17, 17, 9, 0.75], [37, 16, 9, 0.7]], ramp, salt + 1)
  // dark pockets under the front puffs where the branches disappear into the leaves
  c.line(22, 39, 32, 39, ramp[0], 1)
  branch(c, 27, 47, 27, 40, bark[0], bark[1])
  branch(c, 26, 41, 21, 38, bark[0], bark[1])
}

// Oak: big round crown of puffs. Inner 54x46 + outline = 56x48.
sprite("canopy_oak", { w: 54, h: 46, outline: "k" }, (c) => roundCanopy(c, OAK, 11, ["q", "s"]))
// Autumn oak: same build, amber and rust.
sprite("canopy_autumn", { w: 54, h: 46, outline: "k" }, (c) => roundCanopy(c, AUT, 23, ["q", "s"]))

// Blossom: a wider, flatter crown on spreading branches, pink with white petals.
sprite("canopy_blossom", { w: 60, h: 42, outline: "k" }, (c) => {
  const bark = ["y", "A"]
  branch(c, 30, 41, 30, 30, bark[0], bark[1])
  branch(c, 30, 34, 14, 24, bark[0], bark[1])
  branch(c, 30, 32, 47, 22, bark[0], bark[1])
  puffs(c, [[12, 22, 9, 0.1], [48, 20, 9, 0.1], [30, 13, 10, 0.2], [21, 14, 8, 0.2], [40, 13, 8, 0.15]], BLO, 31)
  puffs(c, [[7, 27, 7, 0.4], [53, 26, 7, 0.4], [19, 25, 9, 0.6], [41, 24, 9, 0.55], [30, 22, 9, 0.7], [25, 10, 7, 0.85], [37, 10, 7, 0.8]], BLO, 32)
  branch(c, 30, 41, 30, 33, bark[0], bark[1])
  branch(c, 29, 35, 23, 31, bark[0], bark[1])
  branch(c, 31, 35, 37, 31, bark[0], bark[1])
  // loose petals: white sparkles on the lit side
  const R = rng(5)
  for (let i = 0; i < 26; i++) {
    const x = Math.floor(R() * 60), y = Math.floor(R() * 34)
    if (c.is(x, y, "f") && c.is(x + 1, y, "fg")) c.px(x, y, "h")
  }
})

// Pine: stacked drooping tiers, lit on the left, shadowed under each tier above.
sprite("canopy_pine", { w: 38, h: 62, outline: "k" }, (c) => {
  const ramp = ["7", "8", "9", "a", "b"]
  const cx = 19
  const tiers = [[0, 12, 5], [8, 22, 8], [17, 33, 11], [27, 45, 14], [38, 58, 18]] // top, bottom, half-width
  // trunk in the gaps
  c.rect(cx - 2, 40, 4, 22, "v"); c.vline(cx - 2, 40, 22, "w")
  tiers.forEach(([t0, t1, hw], ti) => {
    for (let y = t0; y <= t1; y++) {
      const f = (y - t0) / (t1 - t0)
      const half = Math.round(1 + (hw - 1) * Math.pow(f, 0.85))
      for (let x = cx - half; x < cx + half; x++) {
        // drooping scalloped hem: the tier's bottom edge curls up between the tips
        const u = (x - (cx - half)) / (2 * half)
        const hem = t1 - Math.round(Math.abs(Math.sin(u * Math.PI * (2 + ti * 0.5))) * 2.5 * f)
        if (y > hem) continue
        const side = (x + 0.5 - cx) / half // -1 left .. 1 right
        const ln = leafNoise(x, y, 70 + ti)
        let lam = -side * 0.8 - (1 - f) * 0.25 + ln * 0.35 + 0.15
        if (y >= hem - 1) lam -= 0.6 // underside of the hem
        const k = lam > 0.75 ? ramp[4] : lam > 0.25 ? ramp[3] : lam > -0.3 ? ramp[2] : lam > -0.8 ? ramp[1] : ramp[0]
        c.px(x, y, k)
      }
    }
  })
  c.px(cx - 1, 0, "b"); c.px(cx, 0, "a")
})

// ---------------------------------------------------------------- trunks + roots

// Trunk segment: centred, `w` px wide (+1px outline), vertical bark grooves that run off both
// ends (so segments stack seamlessly), lit left edge, shaded right edge.
function trunk(c, ox, w, bark, v, salt, knot) {
  const [o, d, m, l, hl] = bark
  const x0 = ox + 8 - Math.floor(w / 2)
  const R = rng(salt + v * 13)
  for (let y = 0; y < 16; y++) {
    c.px(x0 - 1, y, o); c.px(x0 + w, y, o)
    for (let x = 0; x < w; x++) {
      const k = x === 0 ? l : x === 1 && hl ? hl : x === w - 1 ? d : m
      c.px(x0 + x, y, k)
    }
  }
  // grooves
  const grooves = w >= 6 ? [x0 + 2, x0 + 4] : [x0 + 2]
  for (const gx of grooves) {
    for (let y = 0; y < 16; y++) if (!(y > 3 && y < 7 && v === 1 && gx === grooves[0])) c.px(gx, y, d)
    // short breaks with a lit lip, per variant
    const by = 3 + Math.floor(R() * 10)
    c.px(gx, by, m); c.px(gx, by + 1, l === hl ? m : l)
  }
  if (knot) {
    const ky = 5 + v * 3
    c.px(x0 + 2, ky, o); c.px(x0 + 3, ky, o); c.px(x0 + 2, ky + 1, d); c.px(x0 + 3, ky + 1, l)
    c.px(x0 + 1, ky - 1, d)
  }
}
const BARK = {
  oak: ["p", "q", "r", "s", "t"],
  pine: ["p", "u", "v", "w", null],
  blossom: ["x", "y", "z", "A", null],
}
function trunkStrip(name, bark, w, salt, extra) {
  sprite(name, { w: 48, h: 16 }, (c) => {
    for (let v = 0; v < 3; v++) {
      trunk(c, v * 16, w, bark, v, salt, v === 2)
      if (extra) extra(c, v * 16, v)
    }
  })
}
trunkStrip("trunk_oak", BARK.oak, 6, 100)
trunkStrip("trunk_pine", BARK.pine, 4, 200)
// cherry bark: horizontal lenticel dashes
trunkStrip("trunk_blossom", BARK.blossom, 6, 300, (c, ox, v) => {
  for (const y of [3 + v, 9 + v, 14 - v]) { c.px(ox + 6, y, "y"); c.px(ox + 7, y, "y"); c.px(ox + 9, y, "x") }
})
// autumn oak shares the oak bark
trunkStrip("trunk_autumn", BARK.oak, 6, 400)

// Root flare: the trunk widens into the ground with roots creeping sideways.
function roots(c, w, bark) {
  const [o, d, m, l, hl] = bark
  const cx = 16
  for (let y = 0; y < 16; y++) {
    const f = Math.max(0, (y - 6) / 9)
    const half = Math.floor(w / 2) + Math.round(Math.pow(f, 2) * 7)
    const x0 = cx - half, x1 = cx + half
    c.px(x0 - 1, y, o); c.px(x1, y, o)
    for (let x = x0; x < x1; x++) {
      const k = x === x0 ? l : x === x0 + 1 && hl ? hl : x >= x1 - 1 - Math.floor(f * 2) ? d : m
      c.px(x, y, k)
    }
  }
  // crotch shadows where the roots split
  for (const [x, y0] of [[cx - 4, 12], [cx + 3, 11], [cx, 13]]) for (let y = y0; y < 16; y++) c.px(x, y, d)
  c.px(cx - 4, 11, o); c.px(cx + 3, 10, o)
  // creeping surface roots
  c.hline(cx - 13, 15, 4, d); c.hline(cx - 12, 14, 2, m); c.px(cx - 14, 15, o)
  c.hline(cx + 10, 15, 4, d); c.hline(cx + 10, 14, 2, m); c.px(cx + 14, 15, o)
  // grooves continue from the trunk
  c.vline(cx - Math.floor(w / 2) + 2, 0, 9, d)
  if (w >= 6) c.vline(cx - Math.floor(w / 2) + 4, 0, 7, d)
}
sprite("roots_oak", { w: 32, h: 16 }, (c) => roots(c, 6, BARK.oak))
sprite("roots_pine", { w: 32, h: 16 }, (c) => roots(c, 4, BARK.pine))
sprite("roots_blossom", { w: 32, h: 16 }, (c) => roots(c, 6, BARK.blossom))
sprite("roots_autumn", { w: 32, h: 16 }, (c) => roots(c, 6, BARK.oak))

// ---------------------------------------------------------------- decorations (16x16 cells)

// Grass tufts: no outline so they melt into the grass caps.
function tuft(c, blades, salt) {
  const R = rng(salt)
  for (const [x, h, lean] of blades) {
    for (let k = 0; k < h; k++) {
      const bx = x + (k >= h - 2 ? lean : 0)
      c.px(bx, 15 - k, k === h - 1 ? "S" : k > h / 2 ? "R" : k < 2 ? "P" : "Q")
    }
    if (R() < 0.3) c.px(x, 15 - h, "T")
  }
}
sprite("_tuft_a", { w: 16, h: 16 }, (c) => tuft(c, [[5, 4, -1], [6, 6, -1], [7, 8, 0], [8, 7, 1], [9, 5, 1], [10, 3, 1]], 1))
sprite("_tuft_b", { w: 16, h: 16 }, (c) => tuft(c, [[3, 3, -1], [4, 5, -1], [6, 7, 0], [7, 5, 0], [9, 9, 1], [10, 6, 1], [12, 4, 1]], 2))
sprite("_tuft_c", { w: 16, h: 16 }, (c) => tuft(c, [[6, 4, -1], [7, 6, 0], [8, 5, 1], [9, 3, 1]], 3))

// Flowers: stem with two leaves and a 5-petal head, outlined.
function flower(c, petal, dark, center, x, h) {
  c.vline(x, 15 - h, h, "Q")
  c.px(x - 1, 12, "R"); c.px(x - 2, 11, "S"); c.px(x + 1, 10, "R"); c.px(x + 2, 9, "R")
  const y = 15 - h
  c.px(x, y - 1, petal); c.px(x - 1, y, petal); c.px(x + 1, y, dark); c.px(x, y + 1, dark)
  c.px(x - 1, y - 1, petal); c.px(x + 1, y - 1, petal); c.px(x - 1, y + 1, dark); c.px(x + 1, y + 1, dark)
  c.px(x, y, center)
}
sprite("_flower_red", { w: 14, h: 15 }, (c) => flower(c, "B", "C", "D", 6, 8))
sprite("_flower_yellow", { w: 14, h: 15 }, (c) => { flower(c, "D", "E", "W", 5, 7); flower(c, "D", "E", "W", 9, 5) })
sprite("_flower_blue", { w: 14, h: 15 }, (c) => flower(c, "F", "G", "D", 7, 9))

sprite("_pebbles", { w: 14, h: 15 }, (c) => {
  c.ellipse(4, 13, 3, 1, "L"); c.hline(3, 12, 3, "N"); c.hline(2, 14, 5, "M")
  c.ellipse(9, 14, 2, 1, "M"); c.px(8, 13, "L"); c.px(9, 13, "N")
  c.px(12, 14, "L"); c.px(12, 13, "N")
})

sprite("_mushroom_red", { w: 14, h: 15 }, (c) => {
  c.rect(5, 10, 3, 5, "H"); c.vline(7, 10, 5, "I")
  c.ellipse(6, 8, 4, 2, "B"); c.hline(3, 9, 7, "C"); c.hline(3, 10, 7, "C")
  c.rect(3, 9, 7, 2, "C"); c.hline(4, 6, 5, "B")
  c.px(4, 7, "W"); c.px(7, 7, "W"); c.px(8, 8, "W"); c.px(5, 8, "H")
  c.hline(4, 11, 5, "I")
})
sprite("_mushroom_brown", { w: 14, h: 15 }, (c) => {
  // two little caps of different heights
  c.vline(4, 11, 4, "H"); c.vline(5, 11, 4, "I")
  c.hline(2, 10, 6, "J"); c.hline(3, 9, 4, "I"); c.px(3, 9, "H")
  c.vline(9, 13, 2, "H"); c.hline(8, 12, 4, "J"); c.px(8, 12, "I")
})

// Bushes: a low cluster of leaf puffs, optionally with berries.
sprite("_bush", { w: 14, h: 15 }, (c) => {
  puffs(c, [[4, 11, 4, 0.3], [10, 11, 4, 0.3], [7, 9, 5, 0.7]], OAK, 41)
})
sprite("_bush_berry", { w: 14, h: 15 }, (c) => {
  puffs(c, [[4, 11, 4, 0.3], [10, 11, 4, 0.3], [7, 9, 5, 0.7]], OAK, 43)
  for (const [x, y] of [[4, 9], [9, 8], [10, 12], [6, 12]]) { c.px(x, y, "B"); c.px(x + 1, y + 1, "C") }
  c.px(4, 9, "W")
})

// Outlined decor gets 1px of outline (14x15 inner -> 16x17, cropped back to 16x16 by
// shifting up: the outline under the bottom row is dropped so the item sits on the ground).
for (const n of ["flower_red", "flower_yellow", "flower_blue", "pebbles", "mushroom_red", "mushroom_brown", "bush", "bush_berry"]) {
  sprite("_o_" + n, { w: 14, h: 15, outline: "k" }, stamp("_" + n, 0, 0))
}
// DecorKind order: TuftA, TuftB, TuftC, FlowerRed, FlowerYellow, FlowerBlue, Pebbles,
// MushroomRed, MushroomBrown, Bush, BushBerry
const DECOR = ["_tuft_a", "_tuft_b", "_tuft_c", "_o_flower_red", "_o_flower_yellow", "_o_flower_blue",
  "_o_pebbles", "_o_mushroom_red", "_o_mushroom_brown", "_o_bush", "_o_bush_berry"]
sprite("decor", { w: 16 * DECOR.length, h: 16 },
  ...DECOR.map((n, i) => stamp(n, i * 16, 0)))

// Loose leaf block (Tile::Leaves is kept for compatibility; trees use canopy sprites).
sprite("leaves_tile", { w: 16, h: 16 }, (c) => {
  c.rect(0, 0, 16, 16, "2")
  puffs(c, [[4, 4, 5, 0.5], [12, 4, 5, 0.4], [4, 12, 5, 0.4], [12, 12, 5, 0.3], [8, 8, 5, 0.6]], OAK, 51)
})
