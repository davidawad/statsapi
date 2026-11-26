use statsapi_reliefweb::{Client, DisasterFilter, Status};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = DisasterFilter {
        statuses: vec![Status::Ongoing, Status::Alert],
        ..DisasterFilter::default()
    };
    let (found, origin) = Client::new().disasters("PHL", &filter)?;
    println!("{} ongoing disasters via {}", found.len(), origin.as_str());
    Ok(())
}
