//! NUTS region lookup by point-in-polygon over GISCO `GeoJSON` boundaries.

use crate::Error;
use serde_json::Value;

/// A NUTS level served by the regional datasets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NutsLevel {
    /// NUTS 2: basic regions for cohesion policy.
    Two,
    /// NUTS 3: small regions for specific diagnoses.
    Three,
}

impl NutsLevel {
    pub(crate) const fn digit(self) -> u8 {
        match self {
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// A NUTS region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NutsRegion {
    /// NUTS code, such as `DE212`.
    pub id: String,
    /// Region name.
    pub name: String,
    /// Two-letter country code of the region.
    pub country: String,
}

type Ring = Vec<(f64, f64)>;

fn ring(value: &Value) -> Option<Ring> {
    value
        .as_array()?
        .iter()
        .map(|point| Some((point.get(0)?.as_f64()?, point.get(1)?.as_f64()?)))
        .collect()
}

/// Even-odd ray casting over one ring; `x` is longitude, `y` latitude.
fn in_ring(ring: &Ring, x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut previous = ring.last().copied();
    for &(xi, yi) in ring {
        if let Some((xj, yj)) = previous {
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
        }
        previous = Some((xi, yi));
    }
    inside
}

/// A polygon is its outer ring minus its holes.
fn in_polygon(rings: &Value, x: f64, y: f64) -> Option<bool> {
    let mut parsed = rings.as_array()?.iter().map(ring);
    let outer = parsed.next()??;
    if !in_ring(&outer, x, y) {
        return Some(false);
    }
    for hole in parsed {
        if in_ring(&hole?, x, y) {
            return Some(false);
        }
    }
    Some(true)
}

fn contains(geometry: &Value, x: f64, y: f64) -> Option<bool> {
    let coordinates = geometry.get("coordinates")?;
    match geometry.get("type")?.as_str()? {
        "Polygon" => in_polygon(coordinates, x, y),
        "MultiPolygon" => {
            let mut any = false;
            for polygon in coordinates.as_array()? {
                any |= in_polygon(polygon, x, y)?;
            }
            Some(any)
        }
        _ => None,
    }
}

pub(crate) fn region_containing(
    payload: &Value,
    lat: f64,
    lon: f64,
    level: NutsLevel,
) -> Result<Option<NutsRegion>, Error> {
    let features = payload
        .get("features")
        .and_then(Value::as_array)
        .ok_or(Error::Layout("features"))?;
    let wanted = u64::from(level.digit());
    for feature in features {
        let properties = feature.get("properties");
        let property = |key: &str| {
            properties
                .and_then(|properties| properties.get(key))
                .and_then(Value::as_str)
        };
        let level_matches = properties
            .and_then(|properties| properties.get("LEVL_CODE"))
            .is_none_or(|code| code.as_u64() == Some(wanted));
        if !level_matches {
            continue;
        }
        let geometry = feature.get("geometry").ok_or(Error::Layout("geometry"))?;
        if contains(geometry, lon, lat).ok_or(Error::Layout("geometry coordinates"))? {
            let id = property("NUTS_ID").ok_or(Error::Layout("NUTS_ID"))?;
            return Ok(Some(NutsRegion {
                id: id.to_string(),
                name: property("NUTS_NAME").unwrap_or(id).to_string(),
                country: property("CNTR_CODE").unwrap_or_default().to_string(),
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundaries() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/nuts-synthetic.geojson"))
            .expect("fixture is valid JSON")
    }

    #[test]
    fn point_inside_a_polygon_finds_its_region() {
        let region = region_containing(&boundaries(), 0.5, 0.5, NutsLevel::Two)
            .expect("decodes")
            .expect("inside the square");
        assert_eq!(region.id, "XX11");
        assert_eq!(region.country, "XX");
    }

    #[test]
    fn point_in_a_hole_belongs_to_the_inner_region() {
        let region = region_containing(&boundaries(), 5.0, 5.0, NutsLevel::Two)
            .expect("decodes")
            .expect("inside the island");
        assert_eq!(region.id, "XX12");
    }

    #[test]
    fn multipolygon_parts_and_outside_points() {
        let region = region_containing(&boundaries(), 0.5, 20.5, NutsLevel::Two)
            .expect("decodes")
            .expect("second part of the multipolygon");
        assert_eq!(region.id, "XX13");
        assert_eq!(
            region_containing(&boundaries(), 50.0, 50.0, NutsLevel::Two).expect("decodes"),
            None
        );
    }

    #[test]
    fn other_levels_are_skipped() {
        assert_eq!(
            region_containing(&boundaries(), 0.5, 0.5, NutsLevel::Three).expect("decodes"),
            None
        );
    }
}
