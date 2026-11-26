use statsapi_noaa_cdo::{parse_days, parse_normals, parse_stations};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stations = parse_stations(
        include_str!("../tests/fixtures/stations.json"),
        40.74,
        -74.17,
    )?;
    let days = parse_days(include_str!("../tests/fixtures/days.json"))?;
    let normals = parse_normals(include_str!("../tests/fixtures/normals.json"))?;

    println!("NOAA NCEI synthetic sample");
    println!("{:<16} {:>10} {:<18}", "Measure", "Value", "Unit");
    println!("{:<16} {:>10} {:<18}", "Stations", stations.len(), "count");
    println!("{:<16} {:>10} {:<18}", "Daily rows", days.len(), "days");
    println!("{:<16} {:>10} {:<18}", "Normal rows", normals.len(), "days");
    Ok(())
}
