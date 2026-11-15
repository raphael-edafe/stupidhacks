// Desktop Siege: the player, an original little site engineer ("the Builder").
// Amber hard hat, teal work jacket with a reflective vest stripe, navy trousers, brown boots.
// Facing right. Inner canvas 14x38; the 1px outline grows every frame to 16x40.
// Light comes from the top-left: left edges light, right edges and undersides dark.
//
// Build: see ../build.sh (copies the strips to assets/sprites/*.png).

pal({
  k: "#181424", // outline
  // hard hat (amber)
  H: "#a85a1c", h: "#e8962c", y: "#ffd25e", Y: "#fff0b0",
  // skin
  S: "#c47e5e", s: "#efbb92", e: "#1c1830",
  // hair
  d: "#4a2a24",
  // jacket (teal)
  K: "#163f4c", J: "#1f6a72", j: "#2fa39e", t: "#6cd6c4",
  // reflective stripe
  v: "#f6e27a", V: "#c8a83e",
  // trousers (navy)
  Q: "#1e1f38", P: "#2c2e52", p: "#40467a", q: "#5a62a0",
  // boots + belt
  B: "#3a2420", b: "#6e4430", c: "#a06a44",
  // gloves
  g: "#d88a2a", G: "#9a5420",
})

const W = 14, H = 38

// Thick limb from (x0,y0) to (x1,y1), 3 px wide, drawn as stacked rows so it shades left->right.
function limb(c, x0, y0, x1, y1, cols, w = 3) {
  const n = Math.max(1, Math.abs(y1 - y0), Math.abs(x1 - x0))
  for (let i = 0; i <= n; i++) {
    const t = i / n
    const x = Math.round(x0 + (x1 - x0) * t)
    const y = Math.round(y0 + (y1 - y0) * t)
    for (let k = 0; k < w; k++) c.px(x + k, y, cols[Math.min(k, cols.length - 1)])
  }
}

function boot(c, x, y, back) {
  // 4 wide x 4 tall, toe pointing right; sole on the bottom row
  const top = back ? "b" : "c", mid = back ? "B" : "b", sole = "B"
  c.hline(x, y, 3, top)
  c.hline(x, y + 1, 4, mid); c.px(x, y + 1, top)
  c.hline(x, y + 2, 5, mid); c.px(x, y + 2, back ? mid : top)
  c.hline(x, y + 3, 5, sole)
}

function arm(c, sx, sy, hx, hy, back) {
  if (back) {
    limb(c, sx, sy, hx, hy - 1, ["J", "K", "K"])
  } else {
    // dark seam on both sides so the sleeve reads against the jacket
    limb(c, sx - 1, sy + 1, hx - 1, hy - 1, ["K"], 1)
    limb(c, sx, sy, hx, hy - 1, ["t", "j", "J"])
    limb(c, sx + 3, sy + 1, hx + 3, hy - 1, ["K"], 1)
  }
  // glove
  c.rect(hx, hy, 3, 2, back ? "G" : "g")
  if (!back) c.px(hx, hy, "y")
}

// pose: { bob, breath, fl: [footX, lift], bl: [footX, lift], fa: [hx, hy], ba: [hx, hy] }
function builder(c, pose) {
  const b = pose.bob || 0       // whole upper body offset (px down)
  const br = pose.breath || 0   // extra shoulder drop for idle breathing
  const u = b + br
  const hipY = 24 + b

  // --- back arm (behind body) ---
  arm(c, 3, 14 + u, pose.ba[0], pose.ba[1] + u, true)

  // --- back leg ---
  {
    const [fx, lift] = pose.bl
    const ankle = 33 - lift
    limb(c, 4, hipY, fx, ankle, ["p", "P", "Q"])
    boot(c, fx, ankle + 1, true)
  }

  // --- torso / jacket ---
  c.rect(3, 13 + u, 8, 10 - br, "j")
  c.vline(3, 14 + u, 9 - br, "t")         // lit back edge
  c.vline(10, 13 + u, 10 - br, "J")       // shaded front edge
  c.hline(4, 13 + u, 6, "t")              // shoulders catch light
  c.px(3, 13 + u, "j")
  c.hline(3, 22 + b, 8, "J")              // jacket hem shadow
  // reflective vest stripe: vertical strap + band
  c.vline(5, 13 + u, 9 - br, "v")
  c.hline(3, 19 + b, 8, "v")
  c.hline(3, 20 + b, 8, "V")
  c.px(5, 20 + b, "V")
  // collar
  c.hline(5, 12 + u, 4, "S")
  // belt
  c.hline(3, 23 + b, 8, "B")
  c.px(8, 23 + b, "y")

  // hips
  c.rect(3, hipY, 8, 2, "p")
  c.hline(3, hipY, 2, "q")
  c.hline(9, hipY, 2, "P")

  // --- front leg ---
  {
    const [fx, lift] = pose.fl
    const ankle = 33 - lift
    limb(c, 7, hipY, fx, ankle, ["q", "p", "P"])
    boot(c, fx, ankle + 1, false)
  }

  // --- head ---
  const hy = u
  // face
  c.rect(3, 6 + hy, 9, 6, "s")
  c.hline(4, 11 + hy, 7, "s")
  c.px(12, 8 + hy, "s")                    // nose
  c.px(12, 9 + hy, "S")
  c.vline(11, 9 + hy, 2, "S")              // jaw shade
  c.hline(6, 11 + hy, 4, "S")              // chin shadow
  c.px(3, 11 + hy, ".")
  // hair at the back + sideburn
  c.rect(2, 6 + hy, 3, 4, "d")
  c.px(2, 10 + hy, "d"); c.px(3, 10 + hy, "d")
  c.px(5, 6 + hy, "d")
  // ear
  c.px(5, 8 + hy, "S"); c.px(5, 9 + hy, "S"); c.px(6, 9 + hy, "S")
  // eye
  c.vline(9, 7 + hy, 2, "e")
  c.px(8, 7 + hy, "Y")
  // hard hat dome
  c.hline(5, 0 + hy, 5, "h")
  c.hline(3, 1 + hy, 8, "h")
  c.rect(2, 2 + hy, 10, 3, "h")
  c.hline(4, 1 + hy, 2, "y")
  c.hline(3, 2 + hy, 2, "y")
  c.px(3, 3 + hy, "y")
  c.px(5, 0 + hy, "Y")
  c.vline(11, 2 + hy, 3, "H")
  c.px(10, 1 + hy, "H")
  // ridge
  c.vline(7, 0 + hy, 5, "y")
  c.vline(8, 1 + hy, 4, "H")
  // brim, sticking out at the front (right)
  c.hline(1, 5 + hy, 13, "h")
  c.hline(1, 5 + hy, 3, "y")
  c.hline(11, 5 + hy, 3, "H")

  // --- front arm (over body) ---
  arm(c, 7, 14 + u, pose.fa[0], pose.fa[1] + u, false)
}

const IDLE = { bl: [3, 0], fl: [7, 0], fa: [7, 22], ba: [3, 22] }

// Idle: 2 frames, shoulders drop 1px on the exhale.
anim("player_idle", { w: W, h: H, outline: "k", fps: 2, frames: 2 }, (c) => {
  builder(c, { ...IDLE, breath: c.frame })
})

// Walk: 6 frames. contact, down, passing(up), contact, down, passing(up).
const WALK = [
  { bob: 0, fl: [9, 0], bl: [1, 0], fa: [4, 21], ba: [7, 21] },
  { bob: 1, fl: [8, 0], bl: [2, 2], fa: [5, 22], ba: [6, 22] },
  { bob: 0, fl: [6, 0], bl: [5, 3], fa: [7, 22], ba: [4, 22] },
  { bob: 0, fl: [2, 0], bl: [9, 0], fa: [9, 21], ba: [1, 21] },
  { bob: 1, fl: [3, 2], bl: [8, 0], fa: [8, 22], ba: [2, 22] },
  { bob: 0, fl: [5, 3], bl: [6, 0], fa: [7, 22], ba: [4, 22] },
]
anim("player_walk", { w: W, h: H, outline: "k", fps: 12, frames: 6 }, (c) => {
  builder(c, WALK[c.frame])
})

// Jump: front knee up, back leg trailing, front arm thrown up and forward.
sprite("player_jump", { w: W, h: H, outline: "k" }, (c) =>
  builder(c, { fl: [9, 4], bl: [2, 1], fa: [10, 19], ba: [1, 19] }))

// Fall: legs dangling apart, both arms up for balance.
sprite("player_fall", { w: W, h: H, outline: "k" }, (c) =>
  builder(c, { fl: [8, 1], bl: [2, 0], fa: [11, 16], ba: [0, 16] }))
