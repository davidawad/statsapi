use statsapi_socrata::SocrataClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SocrataClient::builder();
    if let Ok(token) = std::env::var("SOCRATA_APP_TOKEN") {
        if !token.trim().is_empty() {
            builder = builder.app_token(token);
        }
    }
    let client = builder.build()?;
    let url = SocrataClient::resource_url(
        "data.cdc.gov",
        "cwsq-ngmh",
        &[("limit".to_string(), "1".to_string())],
    )?;
    for row in client.get_rows(&url)? {
        println!("{}", serde_json::to_string_pretty(&row)?);
    }
    Ok(())
}
