use statsapi_wvs_pew::{pew_aggregate, weighted_wvs, WvsMetric};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wvs = weighted_wvs(
        include_bytes!("../tests/fixtures/wvs_wave7.csv"),
        "AAA",
        WvsMetric::GeneralizedTrust,
        false,
    )?
    .ok_or("WVS fixture has no estimate")?;
    let pew = pew_aggregate(
        include_bytes!("../tests/fixtures/pew_aggregates.csv"),
        "AAA",
        "video_use",
        None,
    )?
    .ok_or("Pew fixture has no aggregate")?;
    let vintage = wvs.vintage.replace('–', "-");

    println!("WVS and Pew synthetic values");
    println!("Measure             Vintage          Value");
    println!(
        "{:<19} {:<16} {:>6.1}",
        "WVS generalized", vintage, wvs.value
    );
    println!(
        "{:<19} {:<16} {:>6.1}",
        "Pew video use", pew.vintage, pew.value
    );
    Ok(())
}
