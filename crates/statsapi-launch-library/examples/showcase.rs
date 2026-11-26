use sha2::{Digest, Sha256};
use statsapi_launch_library::{Client, Filter, Listing};
use std::fs;
use std::time::Duration;

fn cache_fixture(dir: &std::path::Path, url: &str) -> std::io::Result<()> {
    let digest = Sha256::digest(url.as_bytes());
    fs::write(
        dir.join(format!("{digest:x}.json")),
        include_bytes!("../tests/fixtures/count.json"),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("statsapi-launch-showcase-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let base = "https://synthetic.invalid/2.2.0";
    let count_url = format!("{base}/launch/previous/?limit=1&mode=list");
    let next_url = format!("{base}/launch/upcoming/?limit=1&mode=list&status__ids=1,2,5,8");
    cache_fixture(&dir, &count_url)?;
    cache_fixture(&dir, &next_url)?;
    let client = Client::builder("showcase/1.0")
        .base_url(base)
        .fallback_cache(&dir, Duration::from_secs(60))
        .offline(true)
        .build()?;
    let count = client.count(Listing::Previous, &Filter::default())?;
    let next = client.next_launch(&[])?.ok_or("missing synthetic launch")?;

    println!("Launch Library synthetic sample");
    println!("{:<16} {:>10} {:<18}", "Measure", "Value", "Origin");
    println!(
        "{:<16} {:>10} {:<18}",
        "Previous count",
        count.count,
        count.origin.as_str()
    );
    println!(
        "{:<16} {:>10} {:<18}",
        "Next launch",
        next.name.len(),
        next.origin.as_str()
    );
    let _ = fs::remove_dir_all(dir);
    Ok(())
}
