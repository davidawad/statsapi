use statsapi_envirofacts::{parse_npl, summarize_tri};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tri: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/tri_rows.json"))?;
    let npl: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/npl.json"))?;
    let tri_summary = summarize_tri(tri.as_array().ok_or("TRI fixture is not an array")?);
    let sites = parse_npl(&npl, 40.55, -74.36)?;

    println!("EPA Envirofacts synthetic results");
    println!("{:<18} {:>12}", "Measure", "Value");
    println!("{:<18} {:>12}", "TRI facilities", tri_summary.facilities);
    println!(
        "{:<18} {:>12.1}",
        "TRI release lbs", tri_summary.onsite_release_lbs
    );
    println!("{:<18} {:>12}", "NPL sites", sites.len());
    Ok(())
}
