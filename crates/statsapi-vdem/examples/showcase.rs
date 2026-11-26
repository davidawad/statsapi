use statsapi_vdem::latest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = include_bytes!("../tests/fixtures/showcase.csv");
    let liberal = latest(csv, "SYN", "v2x_libdem")?.ok_or("missing liberal democracy value")?;
    let electoral = latest(csv, "SYN", "v2x_polyarchy")?.ok_or("missing electoral value")?;

    println!("V-Dem synthetic values");
    println!("Metric                    Year  Value");
    println!(
        "{:<25} {:>4}  {:>5.2}",
        "Liberal democracy", liberal.1, liberal.0
    );
    println!(
        "{:<25} {:>4}  {:>5.2}",
        "Electoral democracy", electoral.1, electoral.0
    );
    Ok(())
}
