use statsapi_launch_library::{Client, Filter, Listing};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder("my-indicator/1.0 (contact: ops@example.org)").build()?;
    let providers = client.providers("USA")?;
    let filter = Filter {
        provider_ids: providers.ids.clone(),
        net_from: Some("2026-01-01".into()),
        ..Filter::default()
    };
    let launches = client.count(Listing::Previous, &filter)?;
    println!("{} launches ({})", launches.count, launches.source_url);
    if let Some(next) = client.next_launch(&providers.ids)? {
        println!("next: {} at {}", next.name, next.net);
    }
    Ok(())
}
