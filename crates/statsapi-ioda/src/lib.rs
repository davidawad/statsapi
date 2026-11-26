//! Blocking client for the IODA (Internet Outage Detection and Analysis) API.
//!
//! The service is keyless and queried live; nothing is cached or written to disk.
//! Event fragments are merged into outage episodes, and the latest signal value
//! is compared with the median of the preceding days.

use serde_json::Value;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.ioda.inetintel.cc.gatech.edu/v2";
const MAX_ATTEMPTS: u32 = 3;
/// Event fragments closer than this many seconds belong to one episode.
const MERGE_GAP_S: i64 = 3600;
/// The latest signal value is compared only with samples older than this.
const BASELINE_GUARD_S: i64 = 6 * 3600;

/// Errors returned by the client.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The service returned a non-success status.
    Status { url: String, status: u16 },
    /// The response was not the expected JSON.
    Decode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "IODA request failed: {error}"),
            Self::Status { url, status } => write!(f, "IODA returned HTTP {status} for {url}"),
            Self::Decode(message) => write!(f, "IODA response is invalid: {message}"),
        }
    }
}
impl std::error::Error for Error {}

/// Kind of geographic entity IODA tracks that this client reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Country,
    Region,
}
impl EntityType {
    /// Name used in IODA URLs and query strings.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Country => "country",
            Self::Region => "region",
        }
    }
}

/// Measurement datasource behind a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datasource {
    /// BGP-visible routed /24 prefixes.
    Bgp,
    /// Active probing: responsive /24 blocks.
    ActiveProbing,
}
impl Datasource {
    /// Name used in IODA URLs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bgp => "bgp",
            Self::ActiveProbing => "ping-slash24",
        }
    }
}

/// An outage episode: one or more overlapping or adjacent IODA event fragments.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    /// Start, Unix seconds.
    pub from: i64,
    /// End, Unix seconds.
    pub until: i64,
    /// Entity name as IODA spells it.
    pub entity: String,
    /// Datasources that raised events in the episode, sorted and unique.
    pub datasources: Vec<String>,
}
impl Episode {
    /// Duration in seconds.
    #[must_use]
    pub fn duration_s(&self) -> i64 {
        self.until - self.from
    }
    /// Human title, e.g. `Iraq outage (bgp, merit-nt)`.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{} outage ({})", self.entity, self.datasources.join(", "))
    }
}

/// A signal time series.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    /// Timestamp of the first value, Unix seconds.
    pub from: i64,
    /// Seconds between values.
    pub step: i64,
    /// Values; `None` where IODA has no sample yet.
    pub values: Vec<Option<f64>>,
}

/// Latest signal value against its baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// Latest non-missing value.
    pub current: f64,
    /// Median of samples at least six hours older than the latest.
    pub baseline: f64,
    /// Timestamp of the latest value, Unix seconds.
    pub at: i64,
}
impl Comparison {
    /// Current value as a percentage of the baseline.
    #[must_use]
    pub fn percent_of_baseline(&self) -> f64 {
        self.current / self.baseline * 100.0
    }
}

/// Parses an `outages/events` response into merged episodes, oldest first.
///
/// # Errors
/// Returns [`Error::Decode`] when `data` is missing or not an array.
pub fn parse_episodes(body: &str) -> Result<Vec<Episode>, Error> {
    let doc: Value = serde_json::from_str(body).map_err(|e| Error::Decode(e.to_string()))?;
    let data = doc
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Decode("no data array".to_owned()))?;
    let mut fragments: Vec<(i64, i64, String, String)> = data
        .iter()
        .filter_map(|event| {
            Some((
                event.get("from")?.as_i64()?,
                event.get("until")?.as_i64()?,
                event
                    .pointer("/entity/name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                event
                    .get("datasource")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            ))
        })
        .collect();
    fragments.sort();
    let mut episodes: Vec<Episode> = Vec::new();
    for (from, until, entity, source) in fragments {
        match episodes.last_mut() {
            Some(last) if from <= last.until + MERGE_GAP_S => {
                last.until = last.until.max(until);
                if !source.is_empty() && !last.datasources.contains(&source) {
                    last.datasources.push(source);
                    last.datasources.sort();
                }
            }
            _ => episodes.push(Episode {
                from,
                until,
                entity,
                datasources: if source.is_empty() {
                    Vec::new()
                } else {
                    vec![source]
                },
            }),
        }
    }
    Ok(episodes)
}

/// Parses a `signals/raw` response (first series of the first entity).
///
/// # Errors
/// Returns [`Error::Decode`] when the series is absent or malformed.
pub fn parse_signal(body: &str) -> Result<Signal, Error> {
    let bad = |what: &str| Error::Decode(what.to_owned());
    let doc: Value = serde_json::from_str(body).map_err(|e| Error::Decode(e.to_string()))?;
    let series = doc
        .pointer("/data/0/0")
        .ok_or_else(|| bad("no signal series"))?;
    let from = series
        .get("from")
        .and_then(Value::as_i64)
        .ok_or_else(|| bad("signal has no from"))?;
    let step = series
        .get("step")
        .and_then(Value::as_i64)
        .filter(|step| *step > 0)
        .ok_or_else(|| bad("signal has no step"))?;
    let values = series
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("signal has no values"))?
        .iter()
        .map(Value::as_f64)
        .collect();
    Ok(Signal { from, step, values })
}

/// Compares the latest value with the median of the samples at least six hours
/// older. `None` when there is no sample, no baseline, or the baseline is not positive.
#[must_use]
pub fn compare(signal: &Signal) -> Option<Comparison> {
    let stamped = |index: usize| -> Option<i64> {
        Some(signal.from + i64::try_from(index).ok()? * signal.step)
    };
    let (latest_index, current) = signal
        .values
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, value)| value.map(|value| (index, value)))?;
    let at = stamped(latest_index)?;
    let mut older: Vec<f64> = signal
        .values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let value = (*value)?;
            (stamped(index)? <= at - BASELINE_GUARD_S).then_some(value)
        })
        .collect();
    if older.is_empty() {
        return None;
    }
    older.sort_by(f64::total_cmp);
    let mid = older.len() / 2;
    let baseline = if older.len() % 2 == 1 {
        older[mid]
    } else {
        f64::midpoint(older[mid - 1], older[mid])
    };
    (baseline > 0.0).then_some(Comparison {
        current,
        baseline,
        at,
    })
}

/// Blocking IODA client. Every call is a live request; nothing is persisted.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
}
impl Client {
    /// Creates a client for the public IODA API.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }
    /// Creates a client for a service at `base_url` (used by tests).
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn with_base_url(base_url: &str) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
    /// URL of the outage-events query for an entity between two Unix times.
    #[must_use]
    pub fn events_url(&self, kind: EntityType, code: &str, from: i64, until: i64) -> String {
        format!(
            "{}/outages/events?entityType={}&entityCode={code}&from={from}&until={until}&format=ioda&limit=500",
            self.base_url,
            kind.as_str()
        )
    }
    /// URL of the raw-signal query for an entity.
    #[must_use]
    pub fn signal_url(
        &self,
        kind: EntityType,
        code: &str,
        source: Datasource,
        from: i64,
        until: i64,
    ) -> String {
        format!(
            "{}/signals/raw/{}/{code}?from={from}&until={until}&datasource={}",
            self.base_url,
            kind.as_str(),
            source.as_str()
        )
    }
    /// Outage episodes overlapping `[from, until]`, oldest first, clipped to those
    /// that end at or after `from`.
    ///
    /// # Errors
    /// Returns an error for transport, status, or decoding failures.
    pub fn episodes(
        &self,
        kind: EntityType,
        code: &str,
        from: i64,
        until: i64,
    ) -> Result<Vec<Episode>, Error> {
        let body = self.get(&self.events_url(kind, code, from, until))?;
        let mut episodes = parse_episodes(&body)?;
        episodes.retain(|episode| episode.until >= from);
        Ok(episodes)
    }
    /// Raw signal for an entity between two Unix times.
    ///
    /// # Errors
    /// Returns an error for transport, status, or decoding failures.
    pub fn signal(
        &self,
        kind: EntityType,
        code: &str,
        source: Datasource,
        from: i64,
        until: i64,
    ) -> Result<Signal, Error> {
        parse_signal(&self.get(&self.signal_url(kind, code, source, from, until))?)
    }
    /// Looks up the IODA region code for a name inside a country (`US`, `Texas`).
    ///
    /// # Errors
    /// Returns an error for transport, status, or decoding failures.
    pub fn region_code(&self, country: &str, name: &str) -> Result<Option<String>, Error> {
        let url = format!(
            "{}/entities/query?entityType=region&relatedTo=country/{country}&limit=500",
            self.base_url
        );
        let doc: Value =
            serde_json::from_str(&self.get(&url)?).map_err(|e| Error::Decode(e.to_string()))?;
        let data = doc
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Decode("no data array".to_owned()))?;
        Ok(data.iter().find_map(|entity| {
            (entity.get("name")?.as_str()?.eq_ignore_ascii_case(name))
                .then(|| entity.get("code")?.as_str().map(str::to_owned))
                .flatten()
        }))
    }
    fn get(&self, url: &str) -> Result<String, Error> {
        let mut last = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * u64::from(attempt)));
            }
            match self
                .http
                .get(url)
                .header("Accept", "application/json")
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    return response.text().map_err(Error::Http)
                }
                Ok(response) => {
                    let status = response.status();
                    last = Some(Error::Status {
                        url: url.to_owned(),
                        status: status.as_u16(),
                    });
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        break;
                    }
                }
                Err(error) => last = Some(Error::Http(error)),
            }
        }
        Err(last.unwrap_or_else(|| Error::Decode(format!("no response from {url}"))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(from: i64, until: i64, source: &str) -> String {
        format!(
            r#"{{"entity":{{"code":"IQ","name":"Iraq"}},"from":{from},"until":{until},"score":1.0,"datasource":"{source}","method":"median"}}"#
        )
    }

    #[test]
    fn adjacent_fragments_merge_into_one_episode() {
        let body = format!(
            r#"{{"data":[{},{},{}]}}"#,
            event(10_000, 11_000, "merit-nt"),
            event(1_000, 2_000, "bgp"),
            event(11_500, 13_000, "bgp"),
        );
        let episodes = parse_episodes(&body).expect("parses");
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[1].duration_s(), 3_000);
        assert_eq!(episodes[1].datasources, ["bgp", "merit-nt"]);
        assert_eq!(episodes[1].title(), "Iraq outage (bgp, merit-nt)");
    }

    #[test]
    fn empty_and_malformed_event_bodies() {
        assert_eq!(parse_episodes(r#"{"data":[]}"#).expect("ok"), []);
        assert!(parse_episodes(r#"{"data":null}"#).is_err());
        assert!(parse_episodes("<html>").is_err());
    }

    #[test]
    fn comparison_uses_latest_value_and_older_median() {
        let mut values: Vec<Option<f64>> = vec![Some(100.0); 100];
        values.extend([Some(50.0), None, None]);
        let signal = Signal {
            from: 0,
            step: 600,
            values,
        };
        let cmp = compare(&signal).expect("comparison");
        assert!((cmp.percent_of_baseline() - 50.0).abs() < 1e-9);
        assert_eq!(cmp.at, 100 * 600);
    }

    #[test]
    fn comparison_needs_a_baseline() {
        let signal = Signal {
            from: 0,
            step: 600,
            values: vec![Some(5.0), None],
        };
        assert_eq!(compare(&signal), None);
        let zero = Signal {
            from: 0,
            step: 600,
            values: vec![Some(0.0); 50],
        };
        assert_eq!(compare(&zero), None);
    }

    #[test]
    fn signal_parsing_keeps_missing_samples() {
        let body = r#"{"data":[[{"from":60,"step":300,"values":[1,null,3.5]}]]}"#;
        let signal = parse_signal(body).expect("parses");
        assert_eq!(signal.values, [Some(1.0), None, Some(3.5)]);
        assert!(parse_signal(r#"{"data":[]}"#).is_err());
    }
}
