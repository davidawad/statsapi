use statsapi_opensky::{BoundingBox, OpenSkyClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bbox = BoundingBox::around(40.7128, -74.006, 100.0)?;
    let (url, snapshot) = OpenSkyClient::new()?.snapshot(&bbox)?;
    println!("{url}");
    println!(
        "{} aircraft, {} on the ground at {}",
        snapshot.total, snapshot.on_ground, snapshot.time
    );
    Ok(())
}
