use statsapi_ucdp_history::{decode_csv, fetch_archive_member};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = fetch_archive_member(
        "https://ucdp.uu.se/downloads/ucdpprio/ucdp-prio-acd-251.zip",
        "ucdp-prio-acd.csv",
    )?;
    let rows = decode_csv(&csv)?;
    println!("{} rows", rows.len());
    Ok(())
}
