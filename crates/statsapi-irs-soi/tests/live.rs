use statsapi_irs_soi::Client;

#[test]
#[ignore = "live IRS request; run manually when network access is available"]
fn fetches_a_public_zip_dataset() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Client::new();
    let returns = client.zip_metric("10001", "returns")?;
    if returns.is_none() {
        return Err("IRS SOI has no published ZIP 10001 value".into());
    }
    Ok(())
}
