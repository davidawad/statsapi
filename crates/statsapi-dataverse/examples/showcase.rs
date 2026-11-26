use sha2::{Digest, Sha256};
use statsapi_dataverse::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let route = "datasets/:persistentId/";
    let query = [("persistentId", "doi:10.0000/SYNTHETIC")];
    let mut url = reqwest::Url::parse("https://example.test/api/")?.join(route)?;
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    let key = format!("GET {url}");
    let cache_dir = std::env::temp_dir().join(format!("dataverse-showcase-{}", std::process::id()));
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = cache_dir.join(format!("{:x}", Sha256::digest(key.as_bytes())));
    std::fs::write(cache_path, include_bytes!("../tests/fixtures/dataset.json"))?;

    let response = Client::new("https://example.test")?
        .with_cache_dir(&cache_dir)
        .offline()
        .get_json(route, &query)?;
    let file = response
        .pointer("/data/latestVersion/files/0/dataFile/filename")
        .and_then(serde_json::Value::as_str)
        .ok_or("synthetic dataset has no filename")?;
    let _ = std::fs::remove_dir_all(&cache_dir);

    println!("Dataverse synthetic dataset");
    println!("{:<18} {:>8}", "Field", "Value");
    println!(
        "{:<18} {:>8}",
        "Status",
        response["status"].as_str().unwrap_or("?")
    );
    println!("{:<18} {:>8}", "File", file);
    Ok(())
}
