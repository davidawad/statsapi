use statsapi_cfpb::CfpbClient;
use std::path::Path;

#[test]
#[ignore = "makes requests to the live CFPB API and Consumer Credit Trends repository"]
fn fetches_complaint_api_and_credit_trends_file() {
    let client = CfpbClient::new(None::<&Path>).expect("client builds");
    let complaints = client
        .complaints("https://www.consumerfinance.gov/data-research/consumer-complaints/search/api/v1/?size=0&date_received_min=2025-01-01&date_received_max=2025-12-31&product=Credit%20card")
        .expect("CFPB complaint API responds");
    assert!(complaints.total > 0);
    let auto_map = client
        .state_map_value("https://raw.githubusercontent.com/cfpb/consumer-credit-trends-data/main/auto-loans/map_data_AUT.csv", "NJ")
        .expect("CFPB state map is available");
    assert!(auto_map.is_finite());
}
