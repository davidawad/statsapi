use statsapi_faostat::{latest, Dataset, FaostatClient, Selector};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selector = Selector {
        item: "210041".into(),
        element: "6121".into(),
    };
    let (url, rows) = FaostatClient::new()?.fetch(
        Dataset::FoodSecurity,
        "USA",
        std::slice::from_ref(&selector),
    )?;
    println!("{url}");
    if let Some(row) = latest(&rows, &selector) {
        println!("{} {} ({})", row.value, row.unit, row.vintage());
    }
    Ok(())
}
