use statsapi_cdc_wonder::{latest_row, Cause, WonderClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = WonderClient::new()?.with_cache_dir("./cdc-cache");
    let national = client.national_series(Cause::DrugOverdose)?;
    if let Some(row) = latest_row(&national.value) {
        println!(
            "{} age-adjusted drug-overdose deaths per 100k: {:?} ({})",
            row.year,
            row.age_adjusted_rate,
            national.path.as_str()
        );
    }
    let county = client.drug_poisoning_county("34023")?;
    println!("{} county rate: {}", county.value.year, county.value.rate);
    Ok(())
}
