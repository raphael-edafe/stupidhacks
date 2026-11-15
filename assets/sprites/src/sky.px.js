// Desktop Siege: backdrop art for the sky. All original. Painted in daylight colours; the
// game tints them for dusk and night.
//
//   parallax_far   640x120  distant mountains, cool blue-violet with snow caps
//   parallax_mid   640x100  rolling hills with tree silhouettes
//   parallax_near  640x80   dark green treeline
// All three strips tile horizontally (every shape is built from functions periodic in 640px).
//   cloud_0..3  soft chunky clouds, 2-3 shades, transparent outside
//   sun 24x24, moon 20x20, star_3 (3x3), star_5a / star_5b (5x5)

pal({
  // far mountains
  a: "#5c6696", b: "#717cab", c: "#8b96c2", d: "#a8b2d8", e: "#c4cce8", s: "#e8eeff", S: "#ffffff", h: "#9aa6cf",
  // mid hills
  f: "#3f6e6a", g: "#4c7f74", i: "#5d9180", j: "#73a68c", J: "#2f5a5a",
  // near treeline
  k: "#163826", l: "#1e4a2e", m: "#2a5e38", n: "#3a7444", N: "#10281c",
  // clouds
  o: "#ffffff", p: "#eef3fb", q: "#d3dcee", r: "#b0bdda",
  // sun / moon / stars
  t: "#fff8d8", u: "#ffe27a", v: "#ffb83a", w: "#ffd45a",
  x: "#f4f2e6", y: "#d4d2c8", z: "#aeacb4", Z: "#8a8898",
  A: "#ffffff", B: "#cfe0ff", C: "#7a9ae0",
})

const W = 640, TAU = Math.PI * 2
// Periodic pseudo-noise: a sum of sines with integer cycle counts over the strip width.
function wave(x, terms) {
  let v = 0
  for (const [cycles, amp, phase] of terms) v += Math.sin((x / W) * cycles * TAU + phase) * amp
  return v
}
const wrap = (dx) => ((dx % W) + W + W / 2) % W - W / 2
function hash(i, salt) {
  let v = Math.imul(i * 374761393 + salt * 668265263, 1274126177)
  v ^= v >>> 13; v = Math.imul(v, 1103515245); v ^= v >>> 16
  return ((v >>> 0) % 10000) / 10000
}

// ---------------------------------------------------------------- far mountains

// Peaks: [x, height, half-width]. The ridge is the max of the peaks (sharp tops, straight-ish
// flanks) plus a little periodic jaggedness. The face left of each summit takes the light.
const PEAKS = [[40, 104, 70], [130, 78, 55], [205, 112, 80], [300, 70, 60], [370, 96, 66], [455, 116, 85], [545, 82, 60], [600, 64, 50]]
sprite("parallax_far", { w: W, h: 120 }, (c) => {
  for (let x = 0; x < W; x++) {
    let best = -1e9, bi = 0
    PEAKS.forEach(([px, ph, hw], i) => {
      const t = Math.abs(wrap(x - px)) / hw
      const hgt = ph * (1 - Math.pow(t, 0.9))
      if (hgt > best) { best = hgt; bi = i }
    })
    const jag = wave(x, [[37, 2.2, 0.3], [71, 1.4, 1.1], [113, 0.9, 2.0]])
    const top = Math.round(120 - Math.max(best + jag, 18 + wave(x, [[5, 4, 0.5], [13, 2, 1]])))
    const [px, ph] = PEAKS[bi]
    const lit = wrap(x - px) < 0
    const snowLine = 120 - ph + 16 + Math.round(wave(x, [[89, 3, 0.2], [151, 2, 1.7]]))
    for (let y = Math.max(0, top); y < 120; y++) {
      const depth = y - top
      let k
      if (y < snowLine && ph > 75) k = lit ? (depth < 2 ? "S" : "s") : depth < 1 ? "s" : "e"
      else if (y > 96) k = y > 108 ? "d" : "h" // haze at the foot
      else k = lit ? (depth < 2 ? "c" : "b") : depth < 1 ? "b" : "a"
      // gullies: diagonal streaks down the faces
      const gully = (x * 2 + (lit ? -y : y)) % 23 === 0 && depth > 3 && y < 100
      if (gully && k !== "S") k = lit ? "a" : "a"
      c.px(x, y, k)
    }
  }
  // dithered haze band where mountains meet the sky haze
  for (let x = 0; x < W; x++) for (let y = 92; y < 97; y++) if ((x + y) % 2 === 0 && c.is(x, y, "abc")) c.px(x, y, "h")
})

// ---------------------------------------------------------------- mid hills

sprite("parallax_mid", { w: W, h: 100 }, (c) => {
  const ground = []
  for (let x = 0; x < W; x++) ground.push(Math.round(52 + wave(x, [[3, 12, 0.4], [7, 6, 1.3], [17, 2, 2.2]])))
  for (let x = 0; x < W; x++) {
    const slope = ground[(x + 1) % W] - ground[(x + W - 1) % W]
    for (let y = ground[x]; y < 100; y++) {
      const d = y - ground[x]
      c.px(x, y, d < 1 ? (slope <= 0 ? "j" : "i") : d < 4 && slope < 0 ? "i" : y > 85 ? "f" : "g")
    }
  }
  // tree silhouettes along the ridge: round crowns and little pines
  for (let i = 0; i < 64; i++) {
    const x = Math.floor((i + hash(i, 1) * 0.8) * W / 64)
    if (hash(i, 2) < 0.3) continue
    const g = ground[x]
    if (hash(i, 3) < 0.5) {
      const r = 3 + Math.floor(hash(i, 4) * 3)
      const cy = g - r - 1
      for (let y = -r; y <= r; y++) for (let dx = -r; dx <= r; dx++) {
        if (dx * dx + y * y > r * r + 1) continue
        const X = (x + dx + W) % W
        c.px(X, cy + y, dx + y < -r * 0.6 ? "j" : dx + y > r * 0.5 ? "f" : "g")
      }
      c.vline(x, cy + r, 2, "J")
    } else {
      const h = 7 + Math.floor(hash(i, 5) * 6)
      for (let y = 0; y < h; y++) {
        const half = Math.floor(y / 2.4)
        for (let dx = -half; dx <= half; dx++) c.px((x + dx + W) % W, g - h + y, dx < 0 ? "i" : "f")
      }
    }
  }
})

// ---------------------------------------------------------------- near treeline

sprite("parallax_near", { w: W, h: 80 }, (c) => {
  const base = []
  for (let x = 0; x < W; x++) base.push(Math.round(40 + wave(x, [[4, 5, 2.1], [9, 3, 0.4]])))
  for (let x = 0; x < W; x++) for (let y = base[x]; y < 80; y++) c.px(x, y, y > 62 ? "N" : "k")
  // dense crowns: overlapping round tops, lit top-left
  for (let i = 0; i < 90; i++) {
    const x = Math.floor((i + hash(i, 11) * 0.9) * W / 90)
    const r = 6 + Math.floor(hash(i, 12) * 6)
    const cy = base[x] - Math.floor(hash(i, 13) * 8)
    const pine = hash(i, 14) < 0.3
    for (let y = -r - (pine ? r : 0); y <= r; y++) for (let dx = -r; dx <= r; dx++) {
      const inside = pine ? Math.abs(dx) <= Math.max(0, (y + r * 2) / 3.2) && y <= r : dx * dx + y * y <= r * r
      if (!inside) continue
      const X = (x + dx + W) % W, Y = cy + y
      if (Y < 0 || Y >= 80) continue
      const l = -(dx * 0.6 + y * 0.8) / r
      const k = l > 0.55 ? "n" : l > 0.05 ? "m" : l > -0.5 ? "l" : "k"
      c.px(X, Y, k)
    }
  }
})

// ---------------------------------------------------------------- clouds

// Puffs [cx, cy, r]; the bottom is flattened, the top lit, the underside shaded, and the
// outermost ring is semi-transparent so they sit softly in the sky.
function cloud(name, w, h, list, flat) {
  sprite(name, { w, h }, (c) => {
    const val = (x, y) => {
      let best = -1
      for (const [cx, cy, r] of list) {
        const d = Math.hypot((x + 0.5 - cx) / r, (y + 0.5 - cy) / (r * 0.85))
        best = Math.max(best, 1 - d)
      }
      return y >= flat ? -1 : best
    }
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const v = val(x, y)
      if (v < 0) continue
      const above = val(x, y - 3) < 0, below = val(x, y + 3) < 0 || y >= flat - 3
      let k = below ? "r" : above ? "o" : "p"
      if (!below && !above && val(x, y + 6) < 0) k = "q"
      if (v < 0.08) k = k === "r" ? "#b0bdda90" : "#ffffffa0"
      c.px(x, y, k)
    }
  })
}
cloud("cloud_0", 44, 18, [[10, 11, 7], [20, 8, 9], [31, 10, 8], [38, 13, 5]], 17)
cloud("cloud_1", 64, 22, [[10, 15, 7], [22, 10, 10], [35, 8, 11], [48, 12, 9], [57, 16, 6]], 21)
cloud("cloud_2", 88, 26, [[10, 19, 8], [24, 13, 11], [40, 9, 13], [56, 12, 11], [70, 15, 10], [81, 20, 6]], 25)
cloud("cloud_3", 52, 16, [[8, 11, 6], [18, 8, 8], [30, 7, 9], [42, 10, 7], [48, 12, 4]], 15)

// ---------------------------------------------------------------- sun, moon, stars

sprite("sun", { w: 24, h: 24 }, (c) => {
  // soft halo
  for (let y = 0; y < 24; y++) for (let x = 0; x < 24; x++) {
    const d = Math.hypot(x + 0.5 - 12, y + 0.5 - 12)
    if (d < 11.5 && d >= 8) c.px(x, y, (x + y) % 2 === 0 ? "#ffe27a50" : "#ffe27a28")
  }
  // rays
  for (let i = 0; i < 8; i++) {
    const a = (i / 8) * TAU
    const len = i % 2 ? 10 : 11.5
    for (let r = 7.5; r < len; r += 0.5) c.px(Math.floor(12 + Math.cos(a) * r), Math.floor(12 + Math.sin(a) * r), i % 2 ? "v" : "w")
  }
  c.circ(12, 12, 6, "v"); c.circ(12, 12, 5, "u"); c.circ(11, 11, 3, "t")
  c.px(9, 9, "A"); c.px(10, 9, "A")
})

sprite("moon", { w: 20, h: 20 }, (c) => {
  for (let y = 0; y < 20; y++) for (let x = 0; x < 20; x++) {
    const d = Math.hypot(x + 0.5 - 10, y + 0.5 - 10)
    if (d < 9.5 && d >= 8) c.px(x, y, (x + y) % 2 === 0 ? "#cfe0ff40" : "#cfe0ff20")
  }
  c.circ(10, 10, 7, "x")
  // shaded far side (bottom-right)
  for (let y = 0; y < 20; y++) for (let x = 0; x < 20; x++) {
    if (!c.is(x, y, "x")) continue
    const nx = (x + 0.5 - 10) / 7, ny = (y + 0.5 - 10) / 7
    if (nx * 0.6 + ny * 0.8 > 0.55) c.px(x, y, "y")
    if (nx * 0.6 + ny * 0.8 > 0.85) c.px(x, y, "z")
  }
  // craters: dark floor, lit lower-right rim
  for (const [x, y, r] of [[7, 8, 2], [12, 12, 1], [12, 6, 1], [8, 13, 1]]) {
    c.circ(x, y, r, "z"); c.px(x + r, y + r, "A"); c.px(x - r + 1, y - r + 1, "Z")
  }
  c.px(6, 5, "A"); c.px(7, 4, "A")
})

sprite("star_3", ".B.\nBAB\n.B.")
sprite("star_5a", "..C..\n..B..\nCBABC\n..B..\n..C..")
sprite("star_5b", "B...B\n.BAB.\n.AAA.\n.BAB.\nB...B")
