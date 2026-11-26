//! Transparency International CPI results workbook (`.xlsx`).
//!
//! Reads the "CPI Timeseries" sheet, whose header row carries `CPI score
//! <year>` and `Rank <year>` columns; the newest year is the edition and the
//! year before it, when present, is the prior edition.

use crate::{ClientError, Edition, Index, Observation};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::{Read, Seek};

type Row = BTreeMap<usize, String>;

const SHEET_PREFIX: &str = "CPI Timeseries";

fn parse_err(message: &str) -> ClientError {
    ClientError::Parse(format!("CPI workbook: {message}"))
}

fn attr(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.unescape_value().ok())
        .map(std::borrow::Cow::into_owned)
}

fn read_member<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, ClientError> {
    let mut file = zip
        .by_name(name)
        .map_err(|_| parse_err(&format!("missing {name}")))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)
        .map_err(|_| parse_err(&format!("unreadable {name}")))?;
    Ok(out)
}

fn column_index(cell_ref: &str) -> usize {
    cell_ref
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .fold(0, |acc, byte| {
            acc * 26 + usize::from(byte.to_ascii_uppercase() - b'A') + 1
        })
        - 1
}

fn shared_strings(xml: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_reader(xml);
    let (mut out, mut current, mut in_t) = (Vec::new(), String::new(), false);
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(e) if e.local_name().as_ref() == b"si" => current.clear(),
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_t = true,
            Event::Text(text) if in_t => current.push_str(&text.decode().unwrap_or_default()),
            Event::End(e) if e.local_name().as_ref() == b"t" => in_t = false,
            Event::End(e) if e.local_name().as_ref() == b"si" => out.push(current.clone()),
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn sheet_path(workbook: &[u8], rels: &[u8]) -> Result<String, ClientError> {
    let mut reader = Reader::from_reader(workbook);
    let mut rel_id = None;
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == b"sheet" => {
                if attr(&e, b"name").is_some_and(|name| name.starts_with(SHEET_PREFIX)) {
                    rel_id = attr(&e, b"id");
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let rel_id = rel_id.ok_or_else(|| parse_err("no CPI Timeseries sheet"))?;
    let mut reader = Reader::from_reader(rels);
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == b"Relationship" => {
                if attr(&e, b"Id").as_deref() == Some(rel_id.as_str()) {
                    let target = attr(&e, b"Target").ok_or_else(|| parse_err("empty target"))?;
                    return Ok(target
                        .strip_prefix('/')
                        .map_or_else(|| format!("xl/{target}"), str::to_string));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(parse_err("sheet relationship not found"))
}

fn sheet_rows(xml: &[u8], strings: &[String]) -> Vec<Row> {
    let mut reader = Reader::from_reader(xml);
    let mut rows = Vec::new();
    let mut row = Row::new();
    let (mut column, mut shared, mut in_value, mut value) = (0, false, false, String::new());
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(e) => match e.local_name().as_ref() {
                b"row" => row = Row::new(),
                b"c" => {
                    column = attr(&e, b"r").map_or(0, |r| column_index(&r));
                    shared = attr(&e, b"t").as_deref() == Some("s");
                    value.clear();
                }
                b"v" | b"t" => in_value = true,
                _ => {}
            },
            Event::Text(text) if in_value => value.push_str(&text.decode().unwrap_or_default()),
            Event::End(e) => match e.local_name().as_ref() {
                b"v" | b"t" => in_value = false,
                b"c" => {
                    let text = if shared {
                        value
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| strings.get(i).cloned())
                    } else {
                        Some(value.clone())
                    };
                    if let Some(text) = text.filter(|text| !text.is_empty()) {
                        row.insert(column, text);
                    }
                }
                b"row" => rows.push(std::mem::take(&mut row)),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    rows
}

fn year_columns(header: &Row, prefix: &str) -> BTreeMap<u16, usize> {
    header
        .iter()
        .filter_map(|(col, text)| Some((text.trim().strip_prefix(prefix)?.parse().ok()?, *col)))
        .collect()
}

fn cell_f64(row: &Row, col: usize) -> Option<f64> {
    row.get(&col)?.trim().parse().ok()
}

fn cell_rank(row: &Row, col: usize) -> Option<u32> {
    row.get(&col)?.trim().parse().ok()
}

pub(crate) fn parse(bytes: &[u8], url: &str) -> Result<Edition, ClientError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| parse_err("not a zip archive"))?;
    let path = sheet_path(
        &read_member(&mut zip, "xl/workbook.xml")?,
        &read_member(&mut zip, "xl/_rels/workbook.xml.rels")?,
    )?;
    let strings = shared_strings(&read_member(&mut zip, "xl/sharedStrings.xml")?);
    let rows = sheet_rows(&read_member(&mut zip, &path)?, &strings);
    let header_at = rows
        .iter()
        .position(|row| row.get(&1).is_some_and(|cell| cell.trim() == "ISO3"))
        .ok_or_else(|| parse_err("no ISO3 header row"))?;
    let header = &rows[header_at];
    let scores = year_columns(header, "CPI score ");
    let ranks = year_columns(header, "Rank ");
    let year = scores
        .keys()
        .rev()
        .find(|year| ranks.contains_key(year))
        .copied()
        .ok_or_else(|| parse_err("no CPI score/Rank column pair"))?;
    let prior = year - 1;
    let observations: Vec<Observation> = rows[header_at + 1..]
        .iter()
        .filter_map(|row| {
            let iso3 = row.get(&1)?.trim().to_string();
            if iso3.len() != 3 {
                return None;
            }
            Some(Observation {
                iso3,
                score: cell_f64(row, scores[&year])?,
                rank: cell_rank(row, ranks[&year]),
                prior_score: scores.get(&prior).and_then(|col| cell_f64(row, *col)),
                prior_rank: ranks.get(&prior).and_then(|col| cell_rank(row, *col)),
            })
        })
        .collect();
    if observations.is_empty() {
        return Err(parse_err("no country rows"));
    }
    Ok(Edition {
        index: Index::Cpi,
        year,
        url: url.to_string(),
        observations,
    })
}
