//! Installed-app catalog for the Desktop Siege launcher.
//!
//! Scans XDG `.desktop` files (system, user, snap and Flatpak dirs), turns each
//! visible `Type=Application` entry into an [`App`] with a ready-to-spawn argv
//! and a 16x16 straight-alpha RGBA icon, and pins a few favourites first.
//!
//! std + `png` only: used by both the compositor and the game preview.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Desktop name used for `OnlyShowIn` / `NotShowIn`: we behave like GNOME so
/// normal desktop apps show up.
const SHOW_AS: &str = "GNOME";
pub const ICON_SIZE: usize = 16;
/// Entries that never open a window of their own (foot's server mode and its
/// client, which needs that server).
const SKIP_IDS: [&str; 2] = ["foot-server", "footclient"];

/// One launchable app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct App {
    /// Desktop file id, e.g. "firefox_firefox" or "org.gnome.Nautilus".
    pub id: String,
    pub name: String,
    /// 16x16 straight-alpha RGBA (1024 bytes), if a PNG icon was found.
    pub icon: Option<Vec<u8>>,
    /// Favourite slot (0-based, in launcher order) or None.
    pub favourite: Option<u8>,
    /// argv to spawn (field codes stripped, `foot -e` prepended for Terminal=true).
    pub exec: Vec<String>,
    /// Run under `dbus-run-session` so single-instance (D-Bus activated) apps
    /// start a fresh instance in the nested compositor. False for foot, for
    /// Firefox (`--new-instance` does that job) and for snaps: `snap run` must
    /// reach systemd on the real session bus to set up its cgroup, and fails
    /// under a private bus ("... is not a snap cgroup").
    pub needs_dbus_session: bool,
}

impl App {
    /// True for Firefox (any flavour: snap, deb, flatpak).
    pub fn is_firefox(&self) -> bool {
        self.id.to_ascii_lowercase().contains("firefox")
    }
}

// ---------------------------------------------------------------------------
// .desktop parsing
// ---------------------------------------------------------------------------

/// The `[Desktop Entry]` keys we care about.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopEntry {
    pub kind: String,
    pub name: Option<String>,
    pub exec: Option<String>,
    pub icon: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub only_show_in: Option<Vec<String>>,
    pub not_show_in: Vec<String>,
    pub try_exec: Option<String>,
}

impl DesktopEntry {
    /// Should a launcher list this entry?
    pub fn visible(&self) -> bool {
        if self.kind != "Application" || self.no_display || self.hidden {
            return false;
        }
        if self.name.as_deref().is_none_or(str::is_empty) || self.exec.as_deref().is_none_or(str::is_empty) {
            return false;
        }
        if let Some(only) = &self.only_show_in {
            if !only.iter().any(|d| d == SHOW_AS) {
                return false;
            }
        }
        !self.not_show_in.iter().any(|d| d == SHOW_AS)
    }
}

/// Parse the `[Desktop Entry]` group of a .desktop file.
pub fn parse_desktop(text: &str) -> Option<DesktopEntry> {
    let mut in_entry = false;
    let mut seen = false;
    let mut e = DesktopEntry::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            seen |= in_entry;
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let list = |v: &str| v.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect::<Vec<_>>();
        let boolean = |v: &str| v.eq_ignore_ascii_case("true");
        match k {
            "Type" => e.kind = v.to_string(),
            "Name" => e.name = Some(unescape_value(v)),
            "Exec" => e.exec = Some(unescape_value(v)),
            "Icon" => e.icon = Some(unescape_value(v)),
            "Terminal" => e.terminal = boolean(v),
            "NoDisplay" => e.no_display = boolean(v),
            "Hidden" => e.hidden = boolean(v),
            "OnlyShowIn" => e.only_show_in = Some(list(v)),
            "NotShowIn" => e.not_show_in = list(v),
            "TryExec" => e.try_exec = Some(unescape_value(v)),
            _ => {} // includes localised Name[xx]=: we prefer the plain name
        }
    }
    seen.then_some(e)
}

/// Desktop-entry string escapes: `\s \n \t \r \\`.
fn unescape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Split an (already value-unescaped) Exec line into argv: whitespace
/// separates args, double quotes group (with `\" \` \$ \\` escapes inside),
/// field codes are removed (`%f %F %u %U %i %c %k` and the deprecated ones),
/// `%%` becomes `%`. Args that were only a field code are dropped.
pub fn split_exec(exec: &str) -> Vec<String> {
    // (text, quoted) per arg, before field-code expansion.
    let mut raw: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let mut have = false;
    let mut quoted = false;
    let mut in_q = false;
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        if in_q {
            match c {
                '"' => in_q = false,
                '\\' if matches!(it.peek(), Some('"' | '`' | '$' | '\\')) => cur.push(it.next().unwrap_or('\\')),
                _ => cur.push(c),
            }
        } else if c == '"' {
            in_q = true;
            have = true;
            quoted = true;
        } else if c.is_whitespace() {
            if have {
                raw.push((std::mem::take(&mut cur), quoted));
                have = false;
                quoted = false;
            }
        } else {
            cur.push(c);
            have = true;
        }
    }
    if have {
        raw.push((cur, quoted));
    }
    raw.into_iter()
        .filter_map(|(arg, quoted)| {
            let mut out = String::with_capacity(arg.len());
            let mut had_code = false;
            let mut it = arg.chars();
            while let Some(c) = it.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match it.next() {
                    Some('%') => out.push('%'),
                    Some(_) | None => had_code = true, // %f %F %u %U %i %c %k %d %D %n %N %v %m ...
                }
            }
            if out.is_empty() && had_code && !quoted {
                None
            } else {
                Some(out)
            }
        })
        .collect()
}

/// Is `bin` runnable (absolute path, or found on $PATH)?
pub fn which(bin: &str) -> bool {
    if bin.contains('/') {
        return Path::new(bin).is_file();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// Build an [`App`] from a parsed entry (no icon lookup). None if hidden,
/// TryExec is missing or Exec is empty.
pub fn app_from_entry(id: &str, e: &DesktopEntry) -> Option<App> {
    if !e.visible() {
        return None;
    }
    if let Some(t) = &e.try_exec {
        if !t.is_empty() && !which(t) {
            return None;
        }
    }
    let mut exec = split_exec(e.exec.as_deref()?);
    if exec.is_empty() {
        return None;
    }
    if e.terminal {
        let mut v = vec!["foot".to_string(), "-e".to_string()];
        v.append(&mut exec);
        exec = v;
    }
    let needs_dbus_session = needs_private_bus(id, &exec);
    Some(App { id: id.to_string(), name: e.name.clone()?, icon: None, favourite: None, exec, needs_dbus_session })
}

/// See [`App::needs_dbus_session`].
pub fn needs_private_bus(id: &str, exec: &[String]) -> bool {
    let Some(bin) = exec.first() else { return false };
    let snap = bin.starts_with("/snap/") || (bin == "env" && exec.iter().any(|a| a.starts_with("BAMF_DESKTOP_FILE_HINT=/var/lib/snapd")));
    !(is_foot(bin) || snap || id.to_ascii_lowercase().contains("firefox"))
}

fn is_foot(bin: &str) -> bool {
    Path::new(bin).file_name().is_some_and(|n| n == "foot")
}

// ---------------------------------------------------------------------------
// Directory scan
// ---------------------------------------------------------------------------

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The `applications` dirs to scan, highest priority first (XDG: the first
/// dir defining a desktop id wins).
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".local/share")));
    if let Some(h) = &data_home {
        dirs.push(h.join("applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(d).join("applications"));
    }
    dirs.push("/var/lib/snapd/desktop/applications".into());
    dirs.push("/var/lib/flatpak/exports/share/applications".into());
    if let Some(h) = &data_home {
        dirs.push(h.join("flatpak/exports/share/applications"));
    }
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Every `*.desktop` under `dir` (recursively) with its desktop id.
fn desktop_files(dir: &Path) -> Vec<(String, PathBuf)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>, depth: u32) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                if depth < 4 {
                    walk(root, &p, out, depth + 1);
                }
            } else if p.extension().is_some_and(|e| e == "desktop") {
                if let Ok(rel) = p.strip_prefix(root) {
                    let id = rel.to_string_lossy().trim_end_matches(".desktop").replace('/', "-");
                    out.push((id, p));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out, 0);
    out
}

/// Scan `dirs` (priority order) into apps without favourites, sorted by name.
/// Icons are resolved when `icons` is true.
pub fn scan_dirs(dirs: &[PathBuf], icons: bool) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        for (id, path) in desktop_files(dir) {
            // First dir wins, even if that entry is hidden (that's how users hide apps).
            if !seen.insert(id.clone()) || SKIP_IDS.contains(&id.as_str()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Some(entry) = parse_desktop(&text) else { continue };
            let Some(mut app) = app_from_entry(&id, &entry) else { continue };
            if icons {
                app.icon = entry.icon.as_deref().and_then(load_icon);
            }
            apps.push(app);
        }
    }
    sort_by_name(&mut apps);
    apps
}

fn sort_by_name(apps: &mut [App]) {
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
}

/// The full catalog: favourites first (in order, `favourite` = 0..), then
/// everything else sorted by name.
pub fn scan() -> Vec<App> {
    with_favourites(scan_dirs(&application_dirs(), true))
}

/// Pull the favourites to the front (synthesizing Terminal / System Monitor
/// when needed) and number them.
pub fn with_favourites(mut apps: Vec<App>) -> Vec<App> {
    let mut favs: Vec<App> = Vec::new();
    let take = |apps: &mut Vec<App>, pred: &dyn Fn(&App) -> bool| -> Option<App> {
        let i = apps.iter().position(pred)?;
        Some(apps.remove(i))
    };
    // 1. Firefox.
    if let Some(a) = take(&mut apps, &|a| a.is_firefox()) {
        favs.push(a);
    }
    // 2. Terminal (foot).
    match take(&mut apps, &|a| a.id == "foot") {
        Some(mut a) => {
            a.name = "Terminal (foot)".into();
            favs.push(a);
        }
        None if which("foot") => favs.push(App {
            id: "desktop-siege.terminal".into(),
            name: "Terminal (foot)".into(),
            icon: load_icon("foot").or_else(|| load_icon("utilities-terminal")),
            exec: vec!["foot".into()],
            ..Default::default()
        }),
        None => {}
    }
    // 3-5. GNOME apps.
    for id in ["org.gnome.Nautilus", "org.gnome.TextEditor", "org.gnome.Calculator"] {
        if let Some(a) = take(&mut apps, &|a| a.id == id) {
            favs.push(a);
        }
    }
    // 6. System Monitor (htop in foot).
    if which("htop") && which("foot") {
        favs.push(App {
            id: "desktop-siege.system-monitor".into(),
            name: "System Monitor (htop)".into(),
            icon: load_icon("htop").or_else(|| load_icon("utilities-system-monitor")),
            exec: vec!["foot".into(), "-e".into(), "htop".into()],
            ..Default::default()
        });
    }
    for (i, a) in favs.iter_mut().enumerate() {
        a.favourite = Some(i as u8);
    }
    favs.extend(apps);
    favs
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

const THEMES: [&str; 3] = ["hicolor", "Yaru", "Adwaita"];
/// 16x16 first (hand-hinted art beats a downscale), then the larger sizes.
const SIZES: [&str; 7] = ["16x16", "48x48", "32x32", "64x64", "128x128", "256x256", "24x24"];

/// Find an icon file (PNG only) for an `Icon=` value.
pub fn find_icon(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') {
        let p = PathBuf::from(name);
        return (p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) && p.is_file()).then_some(p);
    }
    let name = name.strip_suffix(".png").unwrap_or(name);
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(h) = home() {
        bases.push(h.join(".local/share/icons"));
        bases.push(h.join(".icons"));
    }
    bases.push("/usr/share/icons".into());
    bases.push("/usr/local/share/icons".into());
    bases.push("/var/lib/flatpak/exports/share/icons".into());
    // Size-major so a sharp small icon in any theme beats a big one.
    for size in SIZES {
        for base in &bases {
            for theme in THEMES {
                let p = base.join(theme).join(size).join("apps").join(format!("{name}.png"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let p = PathBuf::from(format!("/usr/share/pixmaps/{name}.png"));
    p.is_file().then_some(p)
}

/// Resolve and decode an icon to 16x16 RGBA.
pub fn load_icon(name: &str) -> Option<Vec<u8>> {
    let path = find_icon(name)?;
    let bytes = std::fs::read(path).ok()?;
    let (w, h, rgba) = decode_png(&bytes)?;
    Some(downscale(&rgba, w, h, ICON_SIZE))
}

/// Decode any PNG to straight-alpha RGBA8.
pub fn decode_png(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let row = buf.get(y * info.line_size..)?;
        for x in 0..w {
            let p = row.get(x * ch..x * ch + ch)?;
            let c = match ch {
                1 => [p[0], p[0], p[0], 255],
                2 => [p[0], p[0], p[0], p[1]],
                3 => [p[0], p[1], p[2], 255],
                _ => [p[0], p[1], p[2], p[3]],
            };
            out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&c);
        }
    }
    Some((w, h, out))
}

/// Box-filter `src` (w x h straight RGBA) into an `n x n` image, keeping the
/// aspect ratio (centred). Colour is averaged premultiplied so transparent
/// pixels don't bleed dark fringes; output is straight alpha.
pub fn downscale(src: &[u8], w: usize, h: usize, n: usize) -> Vec<u8> {
    let mut out = vec![0u8; n * n * 4];
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return out;
    }
    let big = w.max(h);
    let (dw, dh) = (((w * n) / big).max(1), ((h * n) / big).max(1));
    let (ox, oy) = ((n - dw) / 2, (n - dh) / 2);
    for dy in 0..dh {
        let (y0, y1) = (dy * h / dh, ((dy + 1) * h / dh).max(dy * h / dh + 1).min(h));
        for dx in 0..dw {
            let (x0, x1) = (dx * w / dw, ((dx + 1) * w / dw).max(dx * w / dw + 1).min(w));
            let (mut r, mut g, mut b, mut a, mut cnt) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &src[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4];
                    let pa = p[3] as u64;
                    r += p[0] as u64 * pa;
                    g += p[1] as u64 * pa;
                    b += p[2] as u64 * pa;
                    a += pa;
                    cnt += 1;
                }
            }
            let o = ((oy + dy) * n + ox + dx) * 4;
            if a > 0 {
                out[o] = (r / a) as u8;
                out[o + 1] = (g / a) as u8;
                out[o + 2] = (b / a) as u8;
                out[o + 3] = (a / cnt.max(1)) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn exec_field_codes_stripped() {
        assert_eq!(split_exec("/snap/bin/firefox %u"), s(&["/snap/bin/firefox"]));
        assert_eq!(split_exec("nautilus --new-window %U"), s(&["nautilus", "--new-window"]));
        assert_eq!(split_exec("app %f %F %i %c %k --x"), s(&["app", "--x"]));
        assert_eq!(split_exec("printf 100%%"), s(&["printf", "100%"]));
        assert_eq!(split_exec("app --file=%f"), s(&["app", "--file="]));
    }

    #[test]
    fn exec_quoting() {
        assert_eq!(split_exec(r#""/opt/My App/run" --flag"#), s(&["/opt/My App/run", "--flag"]));
        assert_eq!(split_exec(r#"sh -c "echo \"hi\" \$HOME""#), s(&["sh", "-c", "echo \"hi\" $HOME"]));
        assert_eq!(split_exec("  a   b  "), s(&["a", "b"]));
        assert_eq!(split_exec(r#"a """#), s(&["a", ""]));
        // Desktop-level escapes happen first: \s is a space inside one arg? No: it becomes whitespace.
        assert_eq!(unescape_value(r"a\sb\\c"), r"a b\c");
    }

    const BASIC: &str = "[Desktop Entry]\nType=Application\nName=Demo\nName[de]=Vorführung\nExec=demo %U\nIcon=demo\n\n[Desktop Action new]\nName=Other\nExec=other\n";

    #[test]
    fn parse_prefers_unlocalised_name_and_ignores_actions() {
        let e = parse_desktop(BASIC).unwrap();
        assert_eq!(e.name.as_deref(), Some("Demo"));
        assert_eq!(e.exec.as_deref(), Some("demo %U"));
        let a = app_from_entry("demo", &e).unwrap();
        assert_eq!(a.exec, s(&["demo"]));
        assert!(a.needs_dbus_session);
    }

    #[test]
    fn hidden_entries_filtered() {
        for extra in ["NoDisplay=true", "Hidden=true", "OnlyShowIn=KDE;", "NotShowIn=GNOME;", "TryExec=/definitely/not/here"] {
            let e = parse_desktop(&BASIC.replace("Icon=demo", &format!("Icon=demo\n{extra}"))).unwrap();
            assert!(app_from_entry("demo", &e).is_none(), "{extra} should hide");
        }
        let e = parse_desktop(&BASIC.replace("Icon=demo", "Icon=demo\nOnlyShowIn=GNOME;Unity;")).unwrap();
        assert!(app_from_entry("demo", &e).is_some());
        let e = parse_desktop(&BASIC.replace("Type=Application", "Type=Link")).unwrap();
        assert!(app_from_entry("demo", &e).is_none());
    }

    #[test]
    fn terminal_apps_wrapped_in_foot() {
        let e = parse_desktop("[Desktop Entry]\nType=Application\nName=Htop\nExec=htop\nTerminal=true\n").unwrap();
        let a = app_from_entry("htop", &e).unwrap();
        assert_eq!(a.exec, s(&["foot", "-e", "htop"]));
        assert!(!a.needs_dbus_session);
    }

    #[test]
    fn snaps_and_firefox_skip_private_bus() {
        assert!(!needs_private_bus("firefox_firefox", &s(&["/snap/bin/firefox"])));
        assert!(!needs_private_bus("snap-store_snap-store", &s(&["/snap/bin/snap-store"])));
        assert!(!needs_private_bus("x", &s(&["env", "BAMF_DESKTOP_FILE_HINT=/var/lib/snapd/desktop/applications/x.desktop", "/snap/bin/x"])));
        assert!(!needs_private_bus("firefox", &s(&["firefox"])));
        assert!(needs_private_bus("org.gnome.Calculator", &s(&["gnome-calculator"])));
    }

    #[test]
    fn scans_temp_dir_with_dedupe_and_subdirs() {
        let root = std::env::temp_dir().join(format!("apps-test-{}", std::process::id()));
        let (hi, lo) = (root.join("hi"), root.join("lo"));
        std::fs::create_dir_all(hi.join("vendor")).unwrap();
        std::fs::create_dir_all(&lo).unwrap();
        std::fs::write(hi.join("demo.desktop"), BASIC.replace("Name=Demo", "Name=Zed Demo")).unwrap();
        std::fs::write(lo.join("demo.desktop"), BASIC).unwrap(); // shadowed
        std::fs::write(hi.join("vendor/tool.desktop"), "[Desktop Entry]\nType=Application\nName=alpha tool\nExec=\"/opt/a b/tool\" %f\n").unwrap();
        std::fs::write(lo.join("gone.desktop"), BASIC.replace("Icon=demo", "NoDisplay=true")).unwrap();
        std::fs::write(lo.join("notes.txt"), "x").unwrap();
        let apps = scan_dirs(&[hi.clone(), lo.clone()], false);
        let _ = std::fs::remove_dir_all(&root);
        let ids: Vec<_> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["vendor-tool", "demo"]); // sorted by name, case-insensitive
        assert_eq!(apps[1].name, "Zed Demo");
        assert_eq!(apps[0].exec, s(&["/opt/a b/tool"]));
    }

    #[test]
    fn favourites_come_first_in_order() {
        let mk = |id: &str, name: &str| App { id: id.into(), name: name.into(), exec: vec![id.into()], ..Default::default() };
        let apps = vec![mk("org.gnome.Calculator", "Calculator"), mk("aaa", "Aaa"), mk("firefox_firefox", "Firefox"), mk("org.gnome.Nautilus", "Files")];
        let out = with_favourites(apps);
        assert_eq!(out[0].id, "firefox_firefox");
        assert_eq!(out[0].favourite, Some(0));
        let pos = |id: &str| out.iter().position(|a| a.id == id).unwrap();
        assert!(pos("org.gnome.Nautilus") < pos("org.gnome.Calculator"));
        assert_eq!(out.last().unwrap().id, "aaa");
        assert_eq!(out.last().unwrap().favourite, None);
        let favs: Vec<u8> = out.iter().filter_map(|a| a.favourite).collect();
        assert_eq!(favs, (0..favs.len() as u8).collect::<Vec<_>>());
    }

    #[test]
    fn downscale_keeps_colour_at_soft_edges() {
        // 32x32: left half opaque red, right half fully transparent black.
        let mut src = vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..17 {
                src[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let out = downscale(&src, 32, 32, 16);
        let p = &out[8 * 4..8 * 4 + 4]; // the mixed column
        assert_eq!(&p[..3], &[255, 0, 0]);
        assert!(p[3] > 0 && p[3] < 255);
        assert_eq!(out[15 * 4 + 3], 0);
    }
}
