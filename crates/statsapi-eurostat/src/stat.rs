//! JSON-stat decoding for Eurostat dissemination responses.

use crate::Error;
use serde_json::Value;

/// One published value of a series.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Reference period, such as `2024`.
    pub time: String,
    /// The numeric value.
    pub value: f64,
    /// Eurostat status flag (for example `p` provisional, `e` estimated), if any.
    pub flag: Option<String>,
    /// Label of the region the value is for.
    pub geo_label: Option<String>,
    /// Dataset title.
    pub dataset_label: Option<String>,
    /// Dataset last-update timestamp.
    pub updated: Option<String>,
}

pub(crate) fn latest_observation(payload: &Value) -> Result<Option<Observation>, Error> {
    let ids = payload
        .get("id")
        .and_then(Value::as_array)
        .ok_or(Error::Layout("id"))?;
    let sizes = payload
        .get("size")
        .and_then(Value::as_array)
        .ok_or(Error::Layout("size"))?;
    let time_position = ids
        .iter()
        .position(|id| id.as_str() == Some("time"))
        .ok_or(Error::Layout("time dimension"))?;
    let time_size = sizes
        .get(time_position)
        .and_then(Value::as_u64)
        .ok_or(Error::Layout("time size"))?;
    let other_cells: u64 = sizes
        .iter()
        .enumerate()
        .filter(|(position, _)| *position != time_position)
        .map(|(_, size)| size.as_u64().unwrap_or(0))
        .product();
    if other_cells != 1 {
        if other_cells == 0 {
            return Ok(None);
        }
        return Err(Error::Layout("filters leave more than one series"));
    }
    if time_position + 1 != ids.len() {
        return Err(Error::Layout("time is not the last dimension"));
    }
    let times = payload
        .pointer("/dimension/time/category/index")
        .and_then(Value::as_object)
        .ok_or(Error::Layout("time categories"))?;
    let values = payload.get("value");
    let lookup = |index: u64| -> Option<&Value> {
        match values? {
            Value::Object(map) => map.get(&index.to_string()),
            Value::Array(items) => items.get(usize::try_from(index).ok()?),
            _ => None,
        }
    };
    let newest = (0..time_size).rev().find_map(|index| {
        lookup(index)
            .and_then(Value::as_f64)
            .map(|value| (index, value))
    });
    let Some((index, value)) = newest else {
        return Ok(None);
    };
    let time = times
        .iter()
        .find(|(_, position)| position.as_u64() == Some(index))
        .map(|(period, _)| period.clone())
        .ok_or(Error::Layout("time category for value"))?;
    let flag = payload
        .pointer(&format!("/status/{index}"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let geo_label = payload
        .pointer("/dimension/geo/category/label")
        .and_then(Value::as_object)
        .and_then(|labels| labels.values().next())
        .and_then(Value::as_str)
        .map(str::to_string);
    let text = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string);
    Ok(Some(Observation {
        time,
        value,
        flag,
        geo_label,
        dataset_label: text("label"),
        updated: text("updated"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> Value {
        serde_json::from_str(text).expect("fixture is valid JSON")
    }

    #[test]
    fn picks_newest_non_null_period_and_its_flag() {
        let payload = fixture(include_str!("../tests/fixtures/gdp-nuts2.json"));
        let observation = latest_observation(&payload)
            .expect("decodes")
            .expect("has a value");
        assert_eq!(observation.time, "2023");
        assert!((observation.value - 66600.0).abs() < f64::EPSILON);
        assert_eq!(observation.flag.as_deref(), Some("p"));
        assert_eq!(observation.geo_label.as_deref(), Some("Oberbayern"));
    }

    #[test]
    fn empty_series_is_none() {
        let payload = fixture(include_str!("../tests/fixtures/empty.json"));
        assert_eq!(latest_observation(&payload).expect("decodes"), None);
    }

    #[test]
    fn unpinned_filters_are_an_error() {
        let payload = fixture(
            r#"{"id":["geo","time"],"size":[2,1],"value":{"0":1},"dimension":{"time":{"category":{"index":{"2024":0}}}}}"#,
        );
        assert!(latest_observation(&payload).is_err());
    }
}
