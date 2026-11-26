use serde_json::Value;
use statsapi_ookla::{value, Tile, Totals};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw: Value = serde_json::from_str(include_str!("../tests/fixtures/tile.json"))?;
    let tile = Tile {
        quadkey: raw["quadkey"].as_str().ok_or("missing quadkey")?.to_owned(),
        avg_download_kbps: raw["avg_d_kbps"].as_f64().ok_or("missing download")?,
        avg_upload_kbps: raw["avg_u_kbps"].as_f64().ok_or("missing upload")?,
        avg_latency_ms: raw["avg_lat_ms"].as_f64().ok_or("missing latency")?,
        tests: raw["tests"].as_f64().ok_or("missing tests")?,
        devices: raw["devices"].as_f64().ok_or("missing devices")?,
    };
    let mut totals = Totals::default();
    totals.include(&tile);

    println!("Ookla synthetic values");
    println!("{:<16} {:>12}", "Metric", "Value");
    for metric in ["download_mbps", "upload_mbps", "latency_ms", "tests"] {
        println!(
            "{metric:<16} {:>12.1}",
            value(&totals, metric).ok_or("missing metric")?
        );
    }
    Ok(())
}
