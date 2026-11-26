use statsapi_cow::{decode_csv, fetch_archive_member};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = fetch_archive_member(
        "https://correlatesofwar.org/wp-content/uploads/COW-country-codes.zip",
        "COW-country-codes.csv",
    )?;
    let rows = decode_csv(&csv)?;
    println!("{} rows", rows.len());
    Ok(())
}
