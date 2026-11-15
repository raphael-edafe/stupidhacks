// Desktop Siege: night mobs. Original "glitch" creatures in purple / magenta with
// RGB-split scanline artefacts so they pop against dark night terrain.
//
//   mob_crawler: ground "glitch bug", faces RIGHT, 4-frame scuttle, 20x14 per frame.
//   mob_bat:     "bit bat", faces the viewer (symmetric), 3-frame flap, 18x12 per frame.

pal({
  k: "#12081c",
  // shell purples (dark -> light)
  1: "#2c1044", 2: "#521c78", 3: "#8a30b4", 4: "#c058e8", 5: "#f0a0ff",
  // glitch magenta + cyan split
  m: "#ff2ec8", M: "#a01080", c: "#4ef0ff",
  // eyes
  e: "#ffe8fa", E: "#ff4ab4",
  // legs / underside
  L: "#3a1450", l: "#6a2a8a",
})

// ------------------------------------------------------------------ crawler

// Body (inner 18x12): dome shell on the left/middle, armoured head + mandibles at the right.
function crawlerBody(c, bob) {
  const y0 = bob
  // shell dome
  c.ellipse(7, 4 + y0, 7, 4, "3")
  // lit top-left
  for (let y = 0; y < 12; y++) for (let x = 0; x < 18; x++)
    if (c.is(x, y, "3") && (x - 4) ** 2 + (y - 1 - y0) ** 2 * 1.8 < 14) c.px(x, y, "4")
  c.hline(3, 1 + y0, 3, "5"); c.px(2, 2 + y0, "5")
  // underside
  c.hline(1, 7 + y0, 13, "2")
  c.hline(3, 8 + y0, 9, "1")
  // shell seam
  c.vline(7, 0 + y0, 7, "2")
  // glitch: a scanline of the shell torn sideways with an RGB split
  c.hline(0, 4 + y0, 5, "m")
  c.hline(9, 4 + y0, 4, "c")
  c.px(5, 4 + y0, "M")
  // head
  c.rect(13, 3 + y0, 4, 5, "2")
  c.hline(13, 3 + y0, 3, "3")
  c.px(13, 4 + y0, "3")
  c.rect(15, 4 + y0, 2, 2, "E")
  c.px(15, 4 + y0, "e")
  c.px(14, 4 + y0, "1")                   // brow: angry slant
  c.px(16, 3 + y0, "1")
  // mandibles
  c.px(17, 6 + y0, "5"); c.px(17, 7 + y0, "4"); c.px(16, 8 + y0, "4"); c.px(16, 7 + y0, "1")
  // antenna
  c.line(14, 2 + y0, 16, 0, "l", 1)
  c.px(16, 0, "m")
}

// Legs: three, in front of the body. Alternating tripod scuttle over 4 frames.
function crawlerLegs(c, f) {
  const hips = [3, 7, 11]
  hips.forEach((hx, i) => {
    // stride offset per frame, legs alternate
    const s = [[1, 0], [0, 1], [-1, 0], [0, 1]][(f + (i % 2) * 2) % 4]
    const spread = i - 1
    const kx = hx + spread * 2 + s[0], ky = 9 - s[1]
    const fx = hx + spread * 3 + s[0] * 2, fy = 11 - s[1]
    c.line(hx, 8, kx, ky, "3", 1)
    c.line(kx, ky, fx, fy, "2", 1)
    c.px(kx, ky, "4")
    c.px(fx, fy, "m")
  })
}

anim("mob_crawler", { w: 18, h: 12, outline: "k", fps: 10, frames: 4 }, (c) => {
  const f = c.frame
  crawlerBody(c, f % 2)
  crawlerLegs(c, f)
})

// ------------------------------------------------------------------ bat

// Symmetric: draw the left half (x 0..7), mirror to 16 wide. Wings are stepped blocks ("bits").
const WING = [
  // wings up
  `
  3.......
  43......
  233.....
  .2233...
  .12233..
  ..1223..
  ...12...
  ........
  ........
  ........`,
  // wings level
  `
  ........
  ........
  ........
  3.......
  433.....
  23333...
  122233..
  .1.12...
  ........
  ........`,
  // wings down
  `
  ........
  ........
  ........
  ........
  ....33..
  ...2233.
  ..12223.
  .1222...
  .12.2...
  .1......`,
]

anim("mob_bat", { w: 16, h: 10, outline: "k", fps: 12, frames: 3, mx: true }, (c) => {
  const f = c.frame
  // wing (left half)
  const rows = WING[f].trim().split("\n").map((r) => r.trim())
  rows.forEach((r, y) => [...r].forEach((ch, x) => { if (ch !== ".") c.px(x, y, ch) }))
  // body (left half of a 6-wide round body at x 5..7)
  const by = f === 2 ? 1 : 0
  c.px(5, 1 + by, "3"); c.px(5, 0 + by, "2")       // pointy ear
  c.px(4, 0 + by, "2")
  c.rect(5, 2 + by, 3, 5, "3")
  c.px(5, 2 + by, "4"); c.px(6, 2 + by, "4"); c.px(7, 2 + by, "4")
  c.px(5, 3 + by, "4")
  c.px(5, 6 + by, "2"); c.hline(6, 7 + by, 2, "2")
  // eye + glitch fang
  c.px(6, 4 + by, "e"); c.px(7, 4 + by, "E")
  c.px(6, 7 + by, "e"); c.px(7, 6 + by, "1")       // fangs
  // glitch tick on the wing
  const [gx, gy] = [[2, 3], [3, 5], [2, 7]][f]
  c.px(gx, gy, "m")
})
