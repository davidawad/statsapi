use statsapi_wikimedia_hiringlab::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client =
        Client::builder("statsapi-wikimedia-hiringlab-example/0.1 (contact: ops@example.org)")
            .cache_dir("./cache")
            .build()?;
    let title = client.wikipedia_title("Example Town")?;
    let pageviews = client.pageviews(&title, "20260101", "20260131")?;
    println!("{} views from {}", pageviews.total, pageviews.source_url);
    let hiring = client.hiring_lab_observation(true, "US")?;
    println!(
        "Hiring Lab index {} on {}",
        hiring.latest, hiring.latest_date
    );
    Ok(())
}
