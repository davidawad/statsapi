use statsapi_eviction_lab::{Client, NetworkPolicy};

#[test]
#[ignore = "requires live access to Eviction Lab's public ETS download"]
fn fetches_and_decodes_public_city_aggregate() {
    let url = "https://dev.evictionlab.org/uploads/philadelphia_weekly_2020_2021.csv";
    let client = Client::new(None).expect("client should initialize");
    let bytes = client
        .fetch(url, NetworkPolicy::Allow)
        .expect("public ETS CSV should be reachable");
    let result = client
        .latest_week(&bytes, "filings_avg", "")
        .expect("city ETS rows should decode");
    assert!(
        result.current > 0.0,
        "current filing aggregate should be positive"
    );
    println!(
        "public ETS CSV decoded: place=Philadelphia week_date={} source={url}; upstream values suppressed",
        result.week_date
    );
}
