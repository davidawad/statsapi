use statsapi_arcgis::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new().get_json(
        "https://services2.arcgis.com/QUAsjBqieHEMNnZW/ArcGIS/rest/services/FEMA_NRI/FeatureServer/0?f=json",
    )?;
    println!("{}", response["name"]);
    Ok(())
}
