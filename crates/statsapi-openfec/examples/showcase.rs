use serde::Deserialize;
use statsapi_openfec::Committee;

#[derive(Deserialize)]
struct Response<T> {
    results: Vec<T>,
}

#[derive(Deserialize)]
struct ScheduleRow {
    committee_id: String,
    count: u64,
    total: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let committees: Response<Committee> =
        serde_json::from_str(include_str!("../tests/fixtures/committees.json"))?;
    let schedule: Response<ScheduleRow> =
        serde_json::from_str(include_str!("../tests/fixtures/schedule-a-by-zip.json"))?;
    let mut total = 0.0;
    let mut count = 0;
    let mut democratic = 0.0;
    let mut democratic_count = 0;
    for row in schedule.results {
        let committee = committees
            .results
            .iter()
            .find(|committee| committee.committee_id == row.committee_id)
            .ok_or("missing synthetic committee")?;
        if committee.is_candidate_committee() {
            total += row.total;
            count += row.count;
            if committee.party.as_deref() == Some("DEM") {
                democratic += row.total;
                democratic_count += row.count;
            }
        }
    }

    println!("OpenFEC synthetic values");
    println!("{:<16} {:>12} {:>8}", "Metric", "Dollars", "Count");
    println!("{:<16} {:>12.2} {:>8}", "Candidate total", total, count);
    println!(
        "{:<16} {:>12.2} {:>8}",
        "Democratic", democratic, democratic_count
    );
    Ok(())
}
