use statsapi_gdelt::decode_timeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::from_slice(include_bytes!("../tests/fixtures/timelinevolraw.json"))?;
    let points = decode_timeline(&payload)?;

    println!("GDELT DOC timeline synthetic values");
    println!("{:<20} {:>12}", "Date", "Volume");
    for point in points.iter().take(3) {
        println!("{:<20} {:>12.1}", point.date, point.value);
    }
    Ok(())
}
