//! RSF World Press Freedom Index CSV (semicolon-delimited, decimal commas).

use crate::{ClientError, Edition, Index, Observation};

fn parse_err(message: &str) -> ClientError {
    ClientError::Parse(format!("RSF CSV: {message}"))
}

fn number(field: Option<&[u8]>) -> Option<f64> {
    let text = String::from_utf8_lossy(field?).trim().replace(',', ".");
    text.parse().ok()
}

fn rank(field: Option<&[u8]>) -> Option<u32> {
    String::from_utf8_lossy(field?).trim().parse().ok()
}

pub(crate) fn parse(bytes: &[u8], year: u16, url: &str) -> Result<Edition, ClientError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .flexible(true)
        .from_reader(bytes);
    let headers: Vec<String> = reader
        .byte_headers()
        .map_err(|_| parse_err("unreadable header"))?
        .iter()
        .map(|cell| String::from_utf8_lossy(cell).trim().to_string())
        .collect();
    let find = |wanted: &dyn Fn(&str) -> bool| headers.iter().position(|cell| wanted(cell));
    let iso = find(&|cell| cell == "ISO").ok_or_else(|| parse_err("no ISO column"))?;
    let score = find(&|cell| cell == "Score" || cell == format!("Score {year}"))
        .ok_or_else(|| parse_err("no score column for this edition"))?;
    let rank_col = find(&|cell| cell == "Rank").ok_or_else(|| parse_err("no Rank column"))?;
    let prior_rank = find(&|cell| cell == "Rank N-1");
    let prior_score = find(&|cell| cell == "Score N-1");
    let mut observations = Vec::new();
    for record in reader.byte_records().flatten() {
        let code = String::from_utf8_lossy(record.get(iso).unwrap_or_default())
            .trim()
            .to_string();
        let Some(score) = number(record.get(score)) else {
            continue;
        };
        if code.len() != 3 {
            continue;
        }
        observations.push(Observation {
            iso3: code,
            score,
            rank: rank(record.get(rank_col)),
            prior_score: prior_score.and_then(|col| number(record.get(col))),
            prior_rank: prior_rank.and_then(|col| rank(record.get(col))),
        });
    }
    if observations.is_empty() {
        return Err(parse_err("no country rows"));
    }
    Ok(Edition {
        index: Index::Rsf,
        year,
        url: url.to_string(),
        observations,
    })
}
