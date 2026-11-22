use statsapi_who_gho::{latest_observation, IndicatorQuery, WhoGhoClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = IndicatorQuery::new("WHOSIS_000002")?.with_dimension("Dim1", "SEX_BTSX")?;
    let client = WhoGhoClient::new()?.with_cache_dir("./who-cache");
    let (url, payload) = client.fetch(&query, "USA")?;
    println!("{url}");
    if let Some(row) = latest_observation(&payload) {
        println!("{} ({})", row["NumericValue"], row["TimeDim"]);
    }
    Ok(())
}
