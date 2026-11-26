use std::collections::BTreeSet;

use statsapi_fcc_bdc::CountyAvailability;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut summary =
        csv::Reader::from_reader(&include_bytes!("../tests/fixtures/county-summary.csv")[..]);
    let row = summary
        .records()
        .next()
        .ok_or("FCC BDC fixture has no synthetic county row")??;
    let total_units = row
        .get(3)
        .ok_or("FCC BDC fixture lacks total units")?
        .parse()?;
    let share_100_20 = Some(
        row.get(6)
            .ok_or("FCC BDC fixture lacks speed share")?
            .parse()?,
    );

    let mut providers =
        csv::Reader::from_reader(&include_bytes!("../tests/fixtures/provider-summary.csv")[..]);
    let provider_count = providers
        .records()
        .filter_map(Result::ok)
        .filter(|row| row.get(4).and_then(|value| value.parse::<f64>().ok()) > Some(0.0))
        .map(|row| row.get(3).unwrap_or_default().to_owned())
        .collect::<BTreeSet<_>>()
        .len();
    let availability = CountyAvailability {
        as_of: "synthetic".to_owned(),
        total_units,
        share_100_20,
        provider_count,
    };

    println!("FCC Broadband Data Collection synthetic values");
    println!(
        "{:<18} {:>12} {:>12} {:>10}",
        "County", "Units", "100/20", "Providers"
    );
    println!(
        "{:<18} {:>12.0} {:>12.2} {:>10}",
        "00001",
        availability.total_units,
        availability.share_100_20.unwrap_or(0.0),
        availability.provider_count
    );
    Ok(())
}
