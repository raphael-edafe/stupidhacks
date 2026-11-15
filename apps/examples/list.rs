//! Print the scanned app catalog: `cargo run -p apps --example list`.

fn main() {
    let t = std::time::Instant::now();
    let apps = apps::scan();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    for a in &apps {
        let fav = a.favourite.map(|f| format!("*{}", f + 1)).unwrap_or_default();
        println!(
            "{fav:3} {:<34} {:<44} icon={} dbus={} exec={:?}",
            a.name,
            a.id,
            if a.icon.is_some() { "y" } else { "n" },
            if a.needs_dbus_session { "y" } else { "n" },
            a.exec
        );
    }
    let icons = apps.iter().filter(|a| a.icon.is_some()).count();
    println!(
        "{} apps, {} with icons ({:.0}%), {} favourites, scanned in {ms:.0} ms",
        apps.len(),
        icons,
        100.0 * icons as f64 / apps.len().max(1) as f64,
        apps.iter().filter(|a| a.favourite.is_some()).count()
    );
}
