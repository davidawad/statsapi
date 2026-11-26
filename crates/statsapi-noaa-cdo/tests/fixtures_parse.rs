use statsapi_noaa_cdo::{haversine_km, parse_days, parse_normals, parse_stations, Client};

#[test]
fn stations_sort_nearest_first_and_keep_ids() {
    let body = std::fs::read_to_string("tests/fixtures/stations.json").unwrap();
    let stations = parse_stations(&body, 40.74, -74.17).unwrap();
    assert_eq!(stations.len(), 7);
    assert_eq!(stations[0].id, "USC00283704");
    assert!(stations[0].distance_km < stations[1].distance_km);
    assert!(stations.iter().any(|s| s.id == "USW00014734"));
}

#[test]
fn days_parse_numeric_types_and_skip_missing() {
    let body = std::fs::read_to_string("tests/fixtures/days.json").unwrap();
    let days = parse_days(&body).unwrap();
    assert_eq!(days.len(), 30);
    assert_eq!(days[0].date.to_string(), "2026-10-17");
    assert_eq!(days[0].values["TMAX"], 61.0);
    assert!(!days[0].values.contains_key("TAVG"));
}

#[test]
fn normals_are_keyed_by_month_day() {
    let body = std::fs::read_to_string("tests/fixtures/normals.json").unwrap();
    let normals = parse_normals(&body).unwrap();
    assert_eq!(normals.len(), 366);
    assert_eq!(normals["01-01"].tavg, Some(32.5));
    assert_eq!(normals["12-31"].ytd_prcp, Some(46.25));
}

#[test]
fn error_bodies_and_distance() {
    assert!(parse_stations(r#"{"errorMessage":"bad"}"#, 0.0, 0.0).is_err());
    assert!(parse_days("not json").is_err());
    assert!((haversine_km(40.0, -74.0, 41.0, -74.0) - 111.2).abs() < 0.5);
    assert!(Client::citation_url("USW00014734").contains("stations=USW00014734"));
}

#[test]
#[ignore = "queries the live NCEI Access Data Service"]
fn live_newark_station_has_data_and_normals() {
    let client = Client::new().unwrap();
    let end = chrono::Utc::now().date_naive();
    let start = end - chrono::Duration::days(45);
    let stations = client
        .stations_near(40.7357, -74.1724, 0.25, start, end)
        .unwrap();
    assert_ne!(stations.len(), 0);
    let days = client
        .daily_summaries("USW00014734", start, end, "PRCP,TMAX")
        .unwrap();
    assert_ne!(days.len(), 0);
    assert_eq!(client.daily_normals("USW00014734").unwrap().len(), 366);
}
