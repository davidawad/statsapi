use statsapi_nws::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("example-app (you@example.com)");
    let alerts = client.alerts_active(33.58, -101.85)?;
    println!("{} active alerts", alerts.len());
    Ok(())
}
