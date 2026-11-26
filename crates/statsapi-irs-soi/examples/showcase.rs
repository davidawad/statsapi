use statsapi_irs_soi::{parse_migration_metric, parse_zip_metric};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zip = parse_zip_metric(
        include_str!("../tests/fixtures/zip.csv"),
        "12345",
        "returns",
    )?
    .ok_or("missing ZIP metric")?;
    let net = parse_migration_metric(
        include_str!("../tests/fixtures/county-in.csv"),
        include_str!("../tests/fixtures/county-out.csv"),
        "county",
        "99001",
        "net_migration_returns",
    )?
    .ok_or("missing migration metric")?;

    println!("IRS Statistics of Income synthetic sample");
    println!("{:<24} {:>10} {:<10}", "Metric", "Value", "Unit");
    println!("{:<24} {:>10.0} {:<10}", "ZIP returns", zip, "returns");
    println!(
        "{:<24} {:>10.0} {:<10}",
        "County net migration", net, "returns"
    );
    Ok(())
}
