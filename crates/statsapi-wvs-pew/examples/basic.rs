use statsapi_wvs_pew::{weighted_wvs, WvsMetric};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This bundled CSV is synthetic and demonstrates the Wave 7 parser API.
    let bytes = include_bytes!("../tests/fixtures/wvs_wave7.csv");
    let estimate = weighted_wvs(bytes, "AAA", WvsMetric::GeneralizedTrust, false)?;
    println!("{estimate:?}");
    Ok(())
}
