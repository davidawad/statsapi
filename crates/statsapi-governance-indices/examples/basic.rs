use statsapi_governance_indices::{GovernanceClient, Index};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = GovernanceClient::new()?;
    for index in [Index::Cpi, Index::Rsf] {
        let edition = client.latest(index, 2026)?;
        if let Some(row) = edition.country("DNK") {
            println!(
                "{} {}: {} (rank {:?})",
                index.name(),
                edition.year,
                row.score,
                row.rank
            );
        }
    }
    Ok(())
}
