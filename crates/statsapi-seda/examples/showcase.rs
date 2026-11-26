use statsapi_seda::{StringRecord, Table};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rows = include_str!("../tests/fixtures/county-cs.csv").lines();
    let headers = StringRecord::from(
        rows.next()
            .ok_or("missing header")?
            .split(',')
            .collect::<Vec<_>>(),
    );
    let rows: Vec<_> = rows
        .take(4)
        .map(|line| StringRecord::from(line.split(',').collect::<Vec<_>>()))
        .collect();
    let table = Table {
        headers,
        rows,
        source_url: "synthetic://seda".to_string(),
        vintage: "synthetic",
    };

    println!("SEDA synthetic sample");
    println!(
        "{:<8} {:<18} {:<8} {:>8}",
        "County", "Name", "Group", "Score"
    );
    for row in &table.rows {
        println!(
            "{:<8} {:<18} {:<8} {:>8}",
            &row[0], &row[1], &row[3], &row[4]
        );
    }
    Ok(())
}
