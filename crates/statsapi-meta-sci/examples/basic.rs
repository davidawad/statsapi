use statsapi_meta_sci::{Client, Source};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = std::env::temp_dir().join("statsapi-meta-sci-cache");
    let client = Client::new().with_cache_dir(cache);
    let rows = client.rows(Source::County, "06037")?;
    for row in rows.iter().take(5) {
        println!("{}: {}", row.friend, row.score);
    }
    Ok(())
}
