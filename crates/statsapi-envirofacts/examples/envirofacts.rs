use statsapi_envirofacts::{Client, TriArea};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let year = client.latest_tri_year(2026)?;
    let tri = client.tri_summary(&TriArea::County("34023".into()), year)?;
    println!(
        "{year}: {} facilities, {} lbs",
        tri.facilities, tri.onsite_release_lbs
    );
    for site in client
        .npl_sites_near(40.5434, -74.3626, 25.0)?
        .iter()
        .take(3)
    {
        println!("{} {:.1} km", site.name, site.distance_km);
    }
    Ok(())
}
