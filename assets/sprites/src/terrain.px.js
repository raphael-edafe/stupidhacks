// Desktop Siege: terrain tiles. All original.
// Light from the top-left, like every other sprite in the game.
//
// Every solid material is one sheet, 256x64: 4 rows (base variants) x 16 columns (edge masks).
// The column index is a 4-bit mask of which neighbours are EXPOSED (not solid):
//   up = 1, right = 2, down = 4, left = 8.
// Exposed tops get a lit lip, exposed undersides a shadow, every exposed side a 1px dark outline,
// and corners where two exposed sides meet are rounded off (transparent).
// The variants and all 16 edge treatments are generated below; nothing is hand-drawn per tile.
//
// Back walls (wall_dirt, wall_stone): 64x16, 4 variants, darker and flat (recessed).
// Grass: grass_caps (rows = variants, 4 columns = [closed, left open, right open, both open]),
// 16x12 each, drawn 6px above the tile top; grass_sides: left wrap + right wrap, 16x16.

pal({
  k: "#181424",
  // pebbles in the dirt (warm grey)
  T: "#7a6a64", U: "#a39286", X: "#cdbdac",
  // dirt, dark -> light (o = outline)
  o: "#26141c", a: "#4a2a26", b: "#6b3f2f", c: "#8a5739", d: "#a8714a", e: "#c8925f",
  // stone (O = outline)
  O: "#171526", A: "#2f2d44", B: "#48475f", C: "#63637d", D: "#82839c", E: "#a6a9bf",
  // copper ore
  1: "#4e1e1a", 2: "#9a4424", 3: "#d87a38", 4: "#ffc27a",
  // iron ore (silvery, slightly warm)
  5: "#3c3648", 6: "#7c7590", 8: "#a8a1b8", 7: "#d4cee0", W: "#ffffff",
  // crystal ore (cyan, like the screen frames)
  9: "#1a3474", q: "#1f86c4", r: "#56dcff", s: "#d4fcff",
  // planks
  t: "#2a1612", u: "#56321f", v: "#865334", w: "#b07646", x: "#d49e64",
  // bedrock
  F: "#07060b", G: "#141220", H: "#211e2e", I: "#322e45", J: "#48435f",
  // back walls (dirt, stone): low contrast, recessed
  K: "#1e1216", L: "#2a1a1b", M: "#352220", N: "#402a25",
  P: "#13131d", Q: "#1b1b28", R: "#232334", S: "#2c2c40",
  // grass
  f: "#15301f", g: "#205428", h: "#317a34", i: "#4fa240", j: "#86cc58", l: "#c4ec86",
  // flowers
  m: "#ff86b0", n: "#c8406e", p: "#ffe36a", y: "#d0962a", z: "#a8ccff", Z: "#5a78d8",
})

const RAMPS = ["oabcde", "TUX", "OABCDE", "1234", "5687W", "9qrs", "tuvwx", "FGHIJ", "KLMN", "PQRS", "fghijl"]
const where = {}
RAMPS.forEach((r, ri) => [...r].forEach((k, i) => { if (!(k in where)) where[k] = [ri, i] }))
// Step a key n shades along its ramp (never onto the ramp's outline slot 0 unless asked).
function step(k, n, floor = 1) {
  const w = where[k]
  if (!w) return k
  const r = RAMPS[w[0]]
  return r[Math.max(Math.min(w[1] + n, r.length - 1), Math.min(floor, w[1]))]
}

// Small deterministic PRNG.
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
const N = 16
const grid = (k) => Array.from({ length: N }, () => Array(N).fill(k))
const inb = (x, y) => x >= 0 && y >= 0 && x < N && y < N

// ---------------------------------------------------------------- base textures

// Dirt: packed soil. Small bumps catch the light on top and drop a shadow underneath,
// a few dark pores, and the odd buried pebble. Bumps sit on a jittered grid so they are
// evenly spread, and stay inside the tile so random variants always butt up cleanly.
function dirt(v) {
  const R = rng(1000 + v * 77)
  const g = grid("c")
  const bumps = [[".dd", "dcb", ".bb"], ["dd.", "dcb", ".bb"], ["dd", "bb"], ["ddd", "cbb"], [".d", "db", "b."]]
  const cells = []
  for (let cy = 0; cy < 3; cy++) for (let cx = 0; cx < 3; cx++) cells.push([cx, cy])
  for (const [cx, cy] of cells) {
    const r = R()
    if (r < 0.15) continue
    const x0 = 1 + cx * 5 + Math.floor(R() * 3), y0 = 1 + cy * 5 + Math.floor(R() * 3)
    if (r < 0.42) { // pore
      if (inb(x0 + 1, y0 + 1)) { g[y0][x0] = "b"; g[y0 + 1][x0] = "a" }
      continue
    }
    const b = bumps[Math.floor(R() * bumps.length)]
    b.forEach((row, dy) => [...row].forEach((ch, dx) => {
      const x = x0 + dx, y = y0 + dy
      if (ch !== "." && x < 15 && y < 15) g[y][x] = ch
    }))
  }
  // a buried pebble in some variants
  if (v % 2 === 1) {
    const x = 4 + Math.floor(R() * 7), y = 4 + Math.floor(R() * 7)
    g[y][x] = "U"; g[y][x + 1] = "X"; g[y + 1][x] = "T"; g[y + 1][x + 1] = "U"; g[y + 1][x + 2] = "T"
    g[y][x + 2] = "U"; g[y + 2][x] = "b"; g[y + 2][x + 1] = "a"; g[y + 2][x + 2] = "a"
    if (g[y - 1][x + 1] === "c") g[y - 1][x + 1] = "d"
  }
  return g
}

// Rough stone: cobbled plates (Voronoi cells wrapped on the tile) split by dark crevices,
// each plate lit along its top-left rim and shadowed along its bottom-right.
function stoneLike(v, salt, keys) {
  const [crev, dark, mid, lit, hi] = keys
  const R = rng(salt + v * 131)
  const seeds = []
  const n = 4 + (v % 2)
  const wd = (a, b) => { let dx = Math.abs(a[0] - b[0]), dy = Math.abs(a[1] - b[1]); dx = Math.min(dx, 16 - dx); dy = Math.min(dy, 16 - dy); return Math.hypot(dx, dy) }
  for (let t = 0; seeds.length < n && t < 500; t++) {
    const s = [R() * 16, R() * 16, R() < 0.22]
    if (seeds.every((o) => wd(o, s) > 6.2)) seeds.push(s)
  }
  const dist = (x, y, s) => {
    let dx = Math.abs(x + 0.5 - s[0]), dy = Math.abs(y + 0.5 - s[1])
    dx = Math.min(dx, 16 - dx); dy = Math.min(dy, 16 - dy)
    return Math.hypot(dx, dy * 1.15)
  }
  const id = grid(0), cv = grid(false)
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
    let d1 = 1e9, d2 = 1e9, i1 = 0
    seeds.forEach((s, i) => {
      const d = dist(x, y, s)
      if (d < d1) { d2 = d1; d1 = d; i1 = i } else if (d < d2) d2 = d
    })
    id[y][x] = i1
    cv[y][x] = d2 - d1 < 0.85
  }
  const g = grid(mid)
  const at = (x, y) => cv[(y + 16) % 16][(x + 16) % 16]
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
    if (cv[y][x]) { g[y][x] = crev; continue }
    const darkPlate = seeds[id[y][x]][2]
    let k = darkPlate ? dark : mid
    if (at(x, y - 1) || at(x - 1, y)) k = darkPlate ? mid : lit
    else if (at(x, y + 1) || at(x + 1, y)) k = darkPlate ? crev : dark
    g[y][x] = k
  }
  // a glint on the brightest rim of one plate
  for (let t = 0; t < 40; t++) {
    const x = Math.floor(R() * 16), y = Math.floor(R() * 16)
    if (g[y][x] === lit && (at(x, y - 1) && at(x - 1, y))) { g[y][x] = hi; break }
  }
  return g
}
const stone = (v) => stoneLike(v, 2000, ["A", "B", "C", "D", "E"])
const bedrock = (v) => stoneLike(v, 7000, ["F", "G", "H", "I", "J"])

// Ores: stone with one big nugget and one or two small ones embedded in it. Each nugget
// sits in a dark socket, is lit from the top-left and the big one carries a white glint.
const BIG = [
  [".##.", "####", "####", ".##."],
  [".###.", "#####", ".###."],
  [".##..", "####.", ".####", "..##."],
  ["###.", "####", ".###"],
]
const SMALL = [["##", "##"], [".#", "##"], ["##.", ".##"], ["#"]]
function ore(ramp, salt) {
  return (v) => {
    const g = stone(v)
    const R = rng(salt + v * 991)
    const [o, d, m, h] = ramp
    const placed = []
    const want = 2 + (v % 2)
    for (let i = 0; i < 60 && placed.length < want; i++) {
      const shape = placed.length === 0 ? BIG[(v + Math.floor(R() * 2)) % BIG.length] : SMALL[Math.floor(R() * SMALL.length)]
      const w = shape[0].length, hh = shape.length
      const x0 = 2 + Math.floor(R() * (13 - w)), y0 = 2 + Math.floor(R() * (13 - hh))
      const cxn = x0 + w / 2, cyn = y0 + hh / 2
      if (placed.some(([px, py]) => Math.hypot(px - cxn, py - cyn) < 5.5)) continue
      placed.push([cxn, cyn])
      const cells = []
      shape.forEach((row, dy) => [...row].forEach((ch, dx) => { if (ch === "#") cells.push([x0 + dx, y0 + dy]) }))
      const set = new Set(cells.map(([x, y]) => x + "," + y))
      const has = (x, y) => set.has(x + "," + y)
      // dark socket: below/right in deep shadow, above/left a thin rim
      for (const [x, y] of cells) for (const [ex, ey] of [[1, 0], [0, 1], [1, 1], [-1, 0], [0, -1]]) {
        const X = x + ex, Y = y + ey
        if (inb(X, Y) && !has(X, Y)) g[Y][X] = ex + ey > 0 ? o : step(g[Y][X], -1)
      }
      for (const [x, y] of cells) {
        const up = has(x, y - 1), left = has(x - 1, y), down = has(x, y + 1), right = has(x + 1, y)
        g[y][x] = !down || !right ? d : !up || !left ? h : m
      }
      if (placed.length === 1) {
        // glint on the upper-left of the big nugget
        const [gx, gy] = cells.find(([x, y]) => g[y][x] === h && has(x + 1, y) && has(x, y + 1)) || cells[0]
        g[gy][gx] = "W"
      }
    }
    return g
  }
}
const copper = ore("1234", 3000)
const iron = ore("5687", 4000)
const crystal = ore("9qrs", 5000)

// Planks: three boards (4px face + 1px seam), lit top edge, staggered butt joints with nails.
function plank(v) {
  const R = rng(6000 + v * 17)
  const g = grid("w")
  const seams = [4, 9, 14]
  const tops = [0, 5, 10, 15]
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
    g[y][x] = seams.includes(y) ? "u" : tops.includes(y) ? "x" : y === 3 || y === 8 || y === 13 ? "v" : "w"
  }
  const joints = [(3 + v * 5) % 14 + 1, (10 + v * 3) % 14 + 1, (6 + v * 7) % 14 + 1, (12 + v * 5) % 14 + 1]
  for (let b = 0; b < 4; b++) {
    if (b === 3) break
    const top = tops[b], bot = seams[b] - 1
    const x = joints[b]
    for (let y = top; y <= bot; y++) { g[y][x] = "u"; g[y][x - 1] = "v" }
    g[top + 1][x + 1] = "t"
  }
  // grain: one short streak per board
  for (let b = 0; b < 3; b++) {
    const y = tops[b] + 1 + Math.floor(R() * 2)
    const x = Math.floor(R() * 9), len = 3 + Math.floor(R() * 3)
    for (let k = 0; k < len; k++) if (g[y][x + k] === "w") g[y][x + k] = "v"
  }
  return g
}

// ---------------------------------------------------------------- edge treatment

// Rounded corner profile: how many pixels are cut on each of the first rows.
const CUT = [3, 1, 1]
function cutAt(x, y, mask) {
  const U = mask & 1, Rt = mask & 2, D = mask & 4, L = mask & 8
  const c = (cx, cy) => cy < CUT.length && cx < CUT[cy]
  return (U && L && c(x, y)) || (U && Rt && c(15 - x, y)) || (D && L && c(x, 15 - y)) || (D && Rt && c(15 - x, 15 - y))
}
function edged(base, mask, outline) {
  const U = mask & 1, Rt = mask & 2, D = mask & 4, L = mask & 8
  const out = grid(".")
  const cut = (x, y) => !inb(x, y) ? false : cutAt(x, y, mask)
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
    if (cutAt(x, y, mask)) continue
    const k = base[y][x]
    const dT = U ? y : 99, dB = D ? 15 - y : 99, dL = L ? x : 99, dR = Rt ? 15 - x : 99
    const nearCut = cut(x - 1, y) || cut(x + 1, y) || cut(x, y - 1) || cut(x, y + 1)
    if (dT === 0 || dB === 0 || dL === 0 || dR === 0 || nearCut) { out[y][x] = outline; continue }
    const diagCut = cut(x - 1, y - 1) || cut(x + 1, y - 1)
    let s = 0
    if (dT === 1 || (diagCut && U)) s = 2
    else if (dT === 2) s = 1
    else if (dB === 1) s = -2
    else if (dB === 2) s = (x + y) % 2 ? -1 : 0
    if (s === 0 && dL === 1) s = 1
    if (s <= 0 && dR === 1) s = Math.min(s, -1)
    if (s < 0 && cut(x - 1, y + 1) || cut(x + 1, y + 1)) s = Math.min(s, -2)
    out[y][x] = step(k, s)
  }
  return out
}

function paint(c, g, ox, oy) {
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) if (g[y][x] !== ".") c.px(ox + x, oy + y, g[y][x])
}

function sheet16(name, gen, outline) {
  sprite(name, { w: 256, h: 64 }, (c) => {
    for (let v = 0; v < 4; v++) {
      const base = gen(v)
      for (let m = 0; m < 16; m++) paint(c, edged(base, m, outline), m * 16, v * 16)
    }
  })
}

sheet16("tiles_dirt", dirt, "o")
sheet16("tiles_stone", stone, "O")
sheet16("tiles_copper", copper, "O")
sheet16("tiles_iron", iron, "O")
sheet16("tiles_crystal", crystal, "O")
sheet16("tiles_plank", plank, "t")
sheet16("tiles_bedrock", bedrock, "F")

// ---------------------------------------------------------------- back walls

// Stone wall: rough masonry courses (4px, mortar on the bottom row), staggered joints,
// each block faintly lit along its top. Very low contrast so it reads as "behind".
function wallStone(v) {
  const R = rng(8000 + v * 29)
  const g = grid("R")
  for (let course = 0; course < 4; course++) {
    const y0 = course * 4
    const j1 = (course * 5 + v * 3 + Math.floor(R() * 3)) % 8 + 2
    const joints = [j1, j1 + 6 + Math.floor(R() * 3)].filter((x) => x < 15)
    for (let x = 0; x < N; x++) {
      g[y0 + 3][x] = "P"
      g[y0][x] = "S"
    }
    for (const j of joints) {
      for (let y = y0; y < y0 + 3; y++) g[y][j] = "P"
      g[y0][j + 1] = "S"
      if (j - 1 >= 0) for (let y = y0 + 1; y < y0 + 3; y++) g[y][j - 1] = "Q"
    }
    // the block right above mortar is slightly shaded
    for (let x = 0; x < N; x++) if (g[y0 + 2][x] === "R" && R() < 0.5) g[y0 + 2][x] = "Q"
  }
  return g
}

// Dirt wall: packed earth with soft horizontal strata and a few dark root holes.
function wallDirt(v) {
  const R = rng(8500 + v * 31)
  const g = grid("M")
  const strata = [2 + (v % 2), 7, 12 - (v % 2)]
  for (const sy of strata) {
    let x = Math.floor(R() * 4)
    while (x < N) {
      const len = 3 + Math.floor(R() * 5)
      for (let k = 0; k < len && x + k < N; k++) { g[sy][x + k] = "N"; if (sy + 1 < N) g[sy + 1][x + k] = "L" }
      x += len + 1 + Math.floor(R() * 3)
    }
  }
  for (let i = 0; i < 3; i++) {
    const x = 1 + Math.floor(R() * 14), y = 1 + Math.floor(R() * 14)
    g[y][x] = "K"
    if (inb(x + 1, y)) g[y][x + 1] = g[y][x + 1] === "M" ? "L" : g[y][x + 1]
  }
  return g
}

for (const [name, gen] of [["wall_dirt", wallDirt], ["wall_stone", wallStone]]) {
  sprite(name, { w: 64, h: 16 }, (c) => { for (let v = 0; v < 4; v++) paint(c, gen(v), v * 16, 0) })
}

// ---------------------------------------------------------------- grass

const CAP_H = 12, CAP_TOP = 6 // cap rows 0-5 hang above the tile, row 6 = tile row 0
// [left open, right open] for the 4 cap columns
const CAP_ENDS = [[0, 0], [1, 0], [0, 1], [1, 1]]
// grass body top per column next to an open side (follows the rounded corner cut)
const endTop = [3, 1, 1]

function grassCap(v, lo, ro) {
  const R = rng(9000 + v * 53)
  const g = Array.from({ length: CAP_H }, () => Array(N).fill("."))
  const top = (x) => (lo && x < 3 ? endTop[x] : ro && x > 12 ? endTop[15 - x] : 0)
  // body: 3-4 px thick with a ragged lower edge and a soft dirt shadow under it
  const thick = []
  for (let x = 0; x < N; x++) thick.push(3 + (R() < 0.45 ? 1 : 0) + (R() < 0.12 ? 1 : 0))
  for (let x = 0; x < N; x++) {
    const t0 = top(x)
    let t1 = t0 + thick[x]
    if ((lo && x === 0) || (ro && x === 15)) t1 = t0 + 2
    for (let y = t0; y < t1; y++) {
      const d = y - t0
      const k = d === 0 ? "j" : d === 1 ? "i" : y === t1 - 1 ? "g" : "h"
      g[CAP_TOP + y][x] = k
    }
    if (CAP_TOP + t1 < CAP_H) g[CAP_TOP + t1][x] = "b"
  }
  // open ends: the outermost grass column is the darker side colour
  if (lo) for (let y = 0; y < CAP_H; y++) if (g[y][0] !== "." && g[y][0] !== "b") g[y][0] = g[y][0] === "j" ? "i" : "g"
  if (ro) for (let y = 0; y < CAP_H; y++) if (g[y][15] !== "." && g[y][15] !== "b") g[y][15] = "g"
  // blades: a few tufts, tallest in the middle, leaning outwards
  const tufts = 3 + (v % 2)
  for (let i = 0; i < tufts; i++) {
    const cx = 1 + Math.floor((i + R() * 0.8) * 14 / tufts)
    const hmax = 3 + Math.floor(R() * 3)
    for (let dx = -2; dx <= 2; dx++) {
      const x = cx + dx
      if (x < 0 || x > 15) continue
      if ((lo && x < 2) || (ro && x > 13)) continue
      if (R() < 0.25) continue
      let h = hmax - Math.abs(dx) - Math.floor(R() * 2)
      if (h <= 0) continue
      const lean = dx < 0 ? -1 : dx > 0 ? 1 : 0
      const t0 = top(x)
      for (let k = 1; k <= h; k++) {
        const bx = k === h && h >= 3 ? x + lean : x
        if (bx < 0 || bx > 15) continue
        const y = CAP_TOP + t0 - k
        if (y < 0) break
        g[y][bx] = k === h ? "j" : k >= h - 1 ? "i" : "h"
      }
    }
  }
  // short stubble everywhere else
  for (let x = 0; x < N; x++) {
    if ((lo && x < 1) || (ro && x > 14)) continue
    const y = CAP_TOP + top(x) - 1
    if (g[y][x] === "." && R() < 0.55) g[y][x] = "i"
  }
  // flowers on the flowered variants
  if (v >= 3) {
    const heads = v === 3 ? [["m", "n", "p"], ["p", "y", "W"]] : [["z", "Z", "p"], ["W", "z", "p"]]
    const xs = [3 + Math.floor(R() * 3), 9 + Math.floor(R() * 3)]
    xs.forEach((x, i) => {
      const [pet, dk, ctr] = heads[i]
      const h = 3 + Math.floor(R() * 2)
      for (let k = 1; k < h; k++) g[CAP_TOP - k][x] = "h"
      const y = CAP_TOP - h
      g[y][x] = ctr
      g[y][x - 1] = pet; g[y][x + 1] = dk; g[y - 1][x] = pet; g[y + 1][x] = dk
      g[CAP_TOP - 2][x + 1] = "i" // a leaf on the stem
    })
  }
  return g
}

const CAP_VARIANTS = 5 // 0-2 plain, 3-4 flowered
sprite("grass_caps", { w: 16 * 4, h: CAP_H * CAP_VARIANTS }, (c) => {
  for (let v = 0; v < CAP_VARIANTS; v++) CAP_ENDS.forEach(([lo, ro], m) => {
    const g = grassCap(v, lo, ro)
    for (let y = 0; y < CAP_H; y++) for (let x = 0; x < N; x++) if (g[y][x] !== ".") c.px(m * 16 + x, v * CAP_H + y, g[y][x])
  })
})

// Grass creeping down an exposed side of a grass tile (left wrap; the right wrap mirrors it).
function grassSide() {
  const g = grid(".")
  const depth0 = [9, 6, 3] // col 0 outermost, then inwards
  for (let x = 0; x < 3; x++) {
    const t = endTop[x]
    for (let y = t; y < depth0[x]; y++) g[y][x] = x === 0 ? "g" : y < t + 1 ? "i" : "h"
  }
  // drips
  g[9][0] = "g"; g[10][0] = "f"; g[6][1] = "g"; g[3][2] = "g"; g[7][1] = "b"; g[11][0] = "b"; g[4][2] = "b"
  return g
}
sprite("grass_sides", { w: 32, h: 16 }, (c) => {
  const g = grassSide()
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) if (g[y][x] !== ".") { c.px(x, y, g[y][x]); c.px(31 - x, y, g[y][x]) }
})

// Preview of random tiling (not exported to the game).
// Preview of random tiling (not exported to the game).
for (const [nm, gen] of [["dirt", dirt], ["stone", stone], ["copper", copper], ["plank", plank], ["bedrock", bedrock]]) {
  sprite("_tiling_" + nm, { w: 128, h: 96 }, (c) => {
    const R = rng(9)
    const bases = [0, 1, 2, 3].map(gen)
    for (let ty = 0; ty < 6; ty++) for (let tx = 0; tx < 8; tx++) paint(c, bases[Math.floor(R() * 4)], tx * 16, ty * 16)
  })
}
sprite("_tiling_test", { w: 128, h: 96 * 5 + 16 }, ...["dirt", "stone", "copper", "plank", "bedrock"].map((n, i) => stamp("_tiling_" + n, 0, i * 100)))
