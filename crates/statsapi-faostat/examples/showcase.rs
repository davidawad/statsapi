use statsapi_faostat::{filter_csv, latest, Selector};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selector = Selector {
        item: "1717".into(),
        element: "5510".into(),
    };
    let rows = filter_csv(
        include_bytes!("../tests/fixtures/qcl-sample.csv").as_slice(),
        840,
        std::slice::from_ref(&selector),
    )?;
    let row = latest(&rows, &selector).ok_or("no synthetic FAOSTAT row")?;

    println!("FAOSTAT synthetic crop production");
    println!("{:<16} {:<8} {:>10}", "Item", "Year", "Value");
    println!("{:<16} {:<8} {:>10.1}", row.item, row.year, row.value);
    Ok(())
}
