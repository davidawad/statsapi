use statsapi_irs_eo::parse_bmf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let organization = parse_bmf(
        include_bytes!("../tests/fixtures/bmf.csv"),
        Some(123_456_789),
        None,
    )?
    .ok_or("missing synthetic organization")?;

    println!("IRS exempt organizations synthetic sample");
    println!("{:<12} {:<24} {:<8}", "EIN", "Name", "State");
    println!(
        "{:<12} {:<24} {:<8}",
        organization.ein.ok_or("missing EIN")?,
        organization.name.as_deref().ok_or("missing name")?,
        organization.state.as_deref().ok_or("missing state")?
    );
    Ok(())
}
