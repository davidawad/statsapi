use statsapi_cfpb::{parse_complaints, parse_state_map};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let complaints = parse_complaints(include_bytes!("../tests/fixtures/complaints.json"))?;
    let map_value = parse_state_map(include_bytes!("../tests/fixtures/auto-map.csv"), "XY")?;

    println!("CFPB synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!("{:<24} {:>12}", "Complaints", complaints.total);
    println!(
        "{:<24} {:>12.1}%",
        "Top product share",
        complaints.top_product_share * 100.0
    );
    println!("{:<24} {:>12.1}%", "State map value", map_value * 100.0);
    Ok(())
}
