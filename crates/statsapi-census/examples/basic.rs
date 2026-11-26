use std::env;

use statsapi_census::Reporter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reporter = Reporter::new(2023)?;
    let (body, url) = reporter.fetch(&["B01003"], &["04000US06"])?;
    println!("Census Reporter response: {} bytes from {url}", body.len());

    if let Ok(key) = env::var("CENSUS_API_KEY") {
        if let Some(key) = statsapi_census::ApiKey::new(key) {
            let client = statsapi_census::Client::new(key)?;
            let query = statsapi_census::Query {
                year: 2023,
                dataset: "acs/acs5".to_owned(),
                variables: vec!["NAME".to_owned(), "B01003_001E".to_owned()],
                geography: statsapi_census::Geography {
                    for_clause: "state:06".to_owned(),
                    in_clauses: Vec::new(),
                },
            };
            let (body, _) = client.fetch(&query)?;
            println!(
                "Census API response: {} rows",
                statsapi_census::parse_api(&body)?.len()
            );
        }
    }
    Ok(())
}
