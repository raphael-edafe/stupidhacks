// Desktop Siege: tools, the torch tile and the app-chooser icons. All original.
// Light from the top-left. Every asset is drawn 2px smaller and grown by the 1px outline.

pal({
  k: "#181424",
  // steel
  D: "#4a4e68", W: "#8a92b4", w: "#d4dcf0", X: "#ffffff",
  // cyan "data" glow (matches the screen frames)
  c: "#5af0ff", C: "#2aa8c8",
  // gold / amber
  G: "#a8641c", g: "#f0a830", y: "#ffe07a",
  // wood
  B: "#4e3020", b: "#8a5634", L: "#b47a48",
  // flame
  r: "#d8401c", o: "#ff8a1e", Y: "#ffd84a", Z: "#fff6c8",
  // frame steel blue-grey (screens)
  F: "#5a6684", f: "#8a98b8", E: "#3a425c",
  // dark panel
  n: "#141826", N: "#222a3e",
  // greens
  m: "#2a8a3a", M: "#56e070", Q: "#c8ffc8", q: "#1a4a26",
  // paper
  p: "#f4f0e4", P: "#c4beb0", R: "#e8584a",
})

// ---------------------------------------------------------------- tools

// Pickaxe: handle from bottom-left up to the top-right, crescent steel head hugging that corner.
sprite("pickaxe", { w: 12, h: 12, outline: "k" }, (c) => {
  const cx = 2.5, cy = 9.5
  for (let y = 0; y < 12; y++) for (let x = 0; x < 12; x++) {
    const dx = x + 0.5 - cx, dy = y + 0.5 - cy
    const d = Math.hypot(dx, dy)
    const a = Math.atan2(-dy, dx) // 0 = right, 90 = up
    const deg = a * 180 / Math.PI
    if (deg < -4 || deg > 94) continue
    // thickest in the middle (45deg), tapering to points
    const mid = 1 - Math.abs(deg - 45) / 50
    const inner = 9.4 - 2.3 * mid
    if (d >= inner && d <= 9.6) {
      const outer = d > 8.6
      c.px(x, y, outer ? (deg > 45 ? "w" : "W") : (deg > 45 ? "W" : "D"))
    }
  }
  // handle
  c.line(0, 11, 6, 5, "b", 1)
  c.line(1, 11, 7, 5, "B", 1)
  c.line(0, 10, 6, 4, "L", 1)
  // collar where the head meets the handle
  c.px(6, 4, "g"); c.px(7, 4, "G"); c.px(6, 5, "G"); c.px(7, 3, "y")
  c.px(5, 1, "X")
})

// Sword: points up-right. Steel edges with a cyan core, gold guard, wrapped grip.
sprite("sword", { w: 12, h: 12, outline: "k" }, `
  ..........wX
  .........wcW
  ........wcD.
  .......wcD..
  ......wcD...
  .....wcD....
  .yg.wcD.....
  .gGwcD......
  ..GgD.......
  ..bGgG......
  .bB..GG.....
  gB..........`)

// The same sword held straight up (the game rotates it for the other swing angles).
sprite("sword_up", { w: 5, h: 12, outline: "k" }, `
  ..X..
  .wcD.
  .wcD.
  .wcD.
  .wcD.
  .wcD.
  .wcD.
  ywcDG
  ggggG
  ..b..
  ..B..
  .gGG.`)

// Torch tile (16x16): wooden stick, iron band, flickering-looking flame.
sprite("torch", { w: 14, h: 14, outline: "k" }, `
  .......r......
  ......ro......
  .....roor.....
  .....oYYo.....
  ....roYZYr....
  ....oYZZYo....
  .....oYYo.....
  .....DWWD.....
  ......Lb......
  ......Lb......
  ......Lb......
  ......LB......
  ......bB......
  ......bB......`)

// ---------------------------------------------------------------- app icons (16x16)

// A little window: steel frame, title bar, dark client area.
function win(c, client = "n") {
  c.rect(0, 0, 14, 14, "F")
  c.hline(0, 0, 14, "f"); c.vline(0, 0, 14, "f")
  c.rect(0, 0, 14, 3, "E")
  c.hline(0, 0, 14, "F")
  c.px(10, 1, "R"); c.px(12, 1, "c")
  c.rect(1, 3, 12, 10, client)
}

sprite("icon_terminal", { w: 14, h: 14, outline: "k" }, (c) => {
  win(c)
  // ">" prompt and cursor, cyan
  c.px(2, 5, "c"); c.px(3, 6, "c"); c.px(4, 7, "c"); c.px(3, 8, "c"); c.px(2, 9, "c")
  c.hline(6, 9, 4, "c")
  c.hline(6, 5, 5, "C")
  c.hline(6, 7, 3, "C")
})

sprite("icon_files", { w: 14, h: 14, outline: "k" }, (c) => {
  // back of the folder + tab
  c.rect(0, 2, 6, 2, "G")
  c.rect(0, 3, 14, 10, "G")
  // a sheet of paper peeking out
  c.rect(2, 4, 10, 4, "p")
  c.hline(3, 5, 6, "P")
  // front flap, lit from the top
  c.rect(0, 6, 14, 7, "g")
  c.hline(0, 6, 14, "y")
  c.hline(0, 12, 14, "G")
  c.vline(13, 7, 5, "G")
  c.hline(5, 9, 4, "G")
})

sprite("icon_editor", { w: 14, h: 14, outline: "k" }, (c) => {
  // page with a folded corner
  c.rect(1, 0, 10, 14, "p")
  c.vline(10, 3, 11, "P"); c.hline(1, 13, 10, "P")
  c.rect(8, 0, 3, 3, "."); c.px(8, 0, "p"); c.px(9, 1, "P"); c.px(8, 1, "P"); c.px(8, 2, "P"); c.px(9, 2, "P")
  c.px(10, 2, "P")
  for (const [y, w] of [[3, 5], [5, 7], [7, 6], [9, 4]]) c.hline(3, y, w, "W")
  // pencil, diagonal, bottom-right
  c.line(7, 12, 12, 7, "g", 1)
  c.line(8, 12, 13, 7, "G", 1)
  c.px(6, 13, "D"); c.px(7, 13, "L")
  c.px(12, 6, "R"); c.px(13, 6, "R"); c.px(13, 7, "R")
})

sprite("icon_calculator", { w: 14, h: 14, outline: "k" }, (c) => {
  c.rect(1, 0, 12, 14, "F")
  c.vline(1, 0, 14, "f"); c.hline(1, 0, 12, "f")
  c.vline(12, 1, 13, "E"); c.hline(2, 13, 11, "E")
  // display
  c.rect(3, 2, 8, 3, "q")
  c.hline(6, 3, 4, "M")
  // 3x3 keys + tall equals
  for (let j = 0; j < 3; j++) for (let i = 0; i < 3; i++) {
    const x = 3 + i * 2 + (i > 0 ? i : 0) - i, y = 6 + j * 2
    c.px(3 + i * 2, y, "w")
  }
  c.rect(9, 6, 2, 5, "g"); c.px(9, 6, "y")
  c.hline(3, 11, 5, "W")
})

sprite("icon_monitor", { w: 14, h: 14, outline: "k" }, (c) => {
  win(c)
  // grid
  for (let x = 2; x < 13; x += 3) c.vline(x, 4, 8, "N")
  c.hline(1, 8, 12, "N")
  // bars (CPU) and a line graph on top
  const ys = [10, 9, 10, 7, 8, 5, 6, 4, 6, 7, 5, 6]
  for (let i = 0; i < 12; i++) {
    c.vline(1 + i, ys[i] + 1, 12 - ys[i], "q")
  }
  for (let i = 0; i < 11; i++) c.line(1 + i, ys[i], 2 + i, ys[i + 1], "M", 1)
  c.px(8, 4, "Q")
})

sprite("icon_matrix", { w: 14, h: 14, outline: "k" }, (c) => {
  c.rect(0, 0, 14, 14, "n")
  // falling glyph streams: [x, head y, length]
  const cols = [[1, 11, 7], [3, 5, 5], [5, 13, 9], [7, 8, 6], [9, 3, 4], [11, 10, 8], [12, 6, 3]]
  for (const [x, hy, len] of cols) {
    for (let i = 0; i < len; i++) {
      const y = hy - i
      if (y < 0 || y > 13) continue
      if (i % 3 === 2) continue // gaps between glyphs
      c.px(x, y, i === 0 ? "Q" : i < 3 ? "M" : i < 5 ? "m" : "q")
    }
  }
})

