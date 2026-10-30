//! `site`: where the drawing is on Earth (phase 9).

use serde_json::{Map, Value, json};

use cadcraft_doc::arch::{SiteLocation, SiteReference};

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad, point_value},
};

/// Command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("site", "Site Location", run_site).menu(&["Architecture", "Site Location"]).params(
        "{latitude?, longitude?, elevation?, trueNorth?, reference?: {point: [x,y], easting, northing, crs}, northArrowAt?: [x,y], clear?: true}",
    )]
}

const KEYS: [&str; 7] = ["latitude", "longitude", "elevation", "trueNorth", "reference", "northArrowAt", "clear"];

/// A finite number under `key`, or a refusal.
fn number(obj: &Map<String, Value>, key: &str) -> Result<Option<f64>, EngineError> {
    match obj.get(key) {
        None => Ok(None),
        Some(v) => match v.as_f64().filter(|x| x.is_finite()) {
            Some(x) => Ok(Some(x)),
            None => Err(bad("site", format!("`{key}` must be a finite number"))),
        },
    }
}

fn parse_reference(v: &Value) -> Result<SiteReference, EngineError> {
    let obj = v.as_object().ok_or_else(|| bad("site", "`reference` must be an object or null"))?;
    if let Some(k) = obj.keys().find(|k| !["point", "easting", "northing", "crs"].contains(&k.as_str())) {
        return Err(bad("site", format!("unknown `reference` key `{k}`")));
    }
    let point = obj.get("point").and_then(point_value).ok_or_else(|| bad("site", "`reference.point` ([x, y]) is required"))?;
    let easting = number(obj, "easting")?.ok_or_else(|| bad("site", "`reference.easting` (number) is required"))?;
    let northing = number(obj, "northing")?.ok_or_else(|| bad("site", "`reference.northing` (number) is required"))?;
    let crs = match obj.get("crs") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(bad("site", "`reference.crs` must be a string")),
    };
    Ok(SiteReference { point, easting, northing, crs })
}

/// The reply describing the drawing's site.
fn describe(s: &Session) -> Result<Value, EngineError> {
    let model = &s.doc()?.arch;
    let Some(site) = &model.site else { return Ok(json!({ "site": null })) };
    let reference =
        site.reference.as_ref().map(|r| json!({ "point": [r.point.x, r.point.y], "easting": r.easting, "northing": r.northing, "crs": r.crs }));
    let arrow = cadcraft_arch::site::north_arrow(model).map(|a| json!({ "at": [a.at.x, a.at.y], "direction": [a.direction.x, a.direction.y] }));
    Ok(json!({ "site": {
        "latitude": site.latitude,
        "longitude": site.longitude,
        "elevation": site.elevation,
        "trueNorth": site.true_north,
        "reference": reference,
        "northArrow": arrow,
    } }))
}

/// Set, merge into, report or clear the site location. With no keys nothing is written.
fn run_site(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let empty = Map::new();
    let obj = match params {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return Err(bad("site", "params must be an object")),
    };
    if let Some(k) = obj.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(bad("site", format!("unknown key `{k}`")));
    }
    let clear = match obj.get("clear") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad("site", "`clear` must be true or false")),
    };
    if obj.is_empty() {
        return describe(s);
    }
    if clear {
        if obj.len() > 1 {
            return Err(bad("site", "`clear` cannot be combined with other keys"));
        }
        s.doc()?;
        let d = s.doc_mut()?;
        d.arch.site = None;
        d.touch_arch();
        return describe(s);
    }

    let mut site: SiteLocation = s.doc()?.arch.site.clone().unwrap_or_default();
    if let Some(v) = number(obj, "latitude")? {
        site.latitude = v;
    }
    if let Some(v) = number(obj, "longitude")? {
        site.longitude = v;
    }
    if let Some(v) = number(obj, "elevation")? {
        site.elevation = v;
    }
    if let Some(v) = number(obj, "trueNorth")? {
        site.true_north = v;
    }
    match obj.get("reference") {
        None => {}
        Some(Value::Null) => site.reference = None,
        Some(v) => site.reference = Some(parse_reference(v)?),
    }
    match obj.get("northArrowAt") {
        None => {}
        Some(Value::Null) => site.north_arrow_at = None,
        Some(v) => site.north_arrow_at = Some(point_value(v).ok_or_else(|| bad("site", "`northArrowAt` must be [x, y] or null"))?),
    }
    cadcraft_arch::site::validate_site(&site).map_err(|e| bad("site", e))?;

    let d = s.doc_mut()?;
    d.arch.site = Some(site);
    d.touch_arch();
    describe(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrow_dir(r: &Value) -> (f64, f64) {
        let d = &r["site"]["northArrow"]["direction"];
        (d[0].as_f64().unwrap_or(f64::NAN), d[1].as_f64().unwrap_or(f64::NAN))
    }

    #[test]
    fn site_sets_merges_and_clears() {
        let mut s = Session::new();
        assert_eq!(s.execute("site", &json!({})).unwrap()["site"], Value::Null);
        let rev = s.doc().unwrap().arch_revision;
        let r = s.execute("site", &json!({ "latitude": 51.05, "longitude": -114.07, "elevation": 1045.0 })).unwrap();
        assert_eq!(r["site"]["latitude"], json!(51.05));
        assert!(s.doc().unwrap().arch_revision > rev);
        let r = s
            .execute(
                "site",
                &json!({ "trueNorth": 30.0, "reference": { "point": [0, 0], "easting": 500000.0, "northing": 5655000.0, "crs": "EPSG:32611" } }),
            )
            .unwrap();
        assert_eq!(r["site"]["latitude"], json!(51.05), "merge keeps earlier keys");
        assert_eq!(r["site"]["trueNorth"], json!(30.0));
        assert_eq!(r["site"]["reference"]["crs"], json!("EPSG:32611"));
        // Query only: nothing is written.
        let rev = s.doc().unwrap().arch_revision;
        let r = s.execute("site", &json!({})).unwrap();
        assert_eq!(r["site"]["elevation"], json!(1045.0));
        assert_eq!(s.doc().unwrap().arch_revision, rev);
        let r = s.execute("site", &json!({ "reference": null, "northArrowAt": [5, 6] })).unwrap();
        assert_eq!(r["site"]["reference"], Value::Null);
        assert_eq!(r["site"]["northArrow"]["at"], json!([5.0, 6.0]));
        let r = s.execute("site", &json!({ "northArrowAt": null })).unwrap();
        assert!(s.doc().unwrap().arch.site.as_ref().unwrap().north_arrow_at.is_none());
        assert_eq!(r["site"]["trueNorth"], json!(30.0));
        let r = s.execute("site", &json!({ "clear": true })).unwrap();
        assert_eq!(r["site"], Value::Null);
        assert!(s.doc().unwrap().arch.site.is_none());
    }

    #[test]
    fn site_refuses_bad_values_and_changes_nothing() {
        let mut s = Session::new();
        s.execute("site", &json!({ "latitude": 10.0 })).unwrap();
        let before = s.doc().unwrap().arch.site.clone();
        let rev = s.doc().unwrap().arch_revision;
        let bads = [
            json!({ "latitude": 91.0 }),
            json!({ "longitude": 181.0 }),
            json!({ "latitude": "x" }),
            json!({ "elevation": f64::NAN }),
            json!({ "bogus": 1 }),
            json!({ "reference": 5 }),
            json!({ "reference": { "point": [0, 0], "easting": 1.0 } }),
            json!({ "northArrowAt": "nope" }),
            json!({ "clear": 1 }),
            json!({ "clear": true, "latitude": 1.0 }),
            json!({ "latitude": 20.0, "trueNorth": "n" }),
        ];
        for p in &bads {
            assert!(s.execute("site", p).is_err(), "should refuse {p}");
        }
        assert_eq!(s.doc().unwrap().arch.site, before);
        assert_eq!(s.doc().unwrap().arch_revision, rev);
    }

    #[test]
    fn site_reports_the_north_arrow() {
        let mut s = Session::new();
        let r = s.execute("site", &json!({ "latitude": 51.05, "trueNorth": 30.0 })).unwrap();
        let (x, y) = arrow_dir(&r);
        assert!((x + 0.5).abs() < 1e-6 && (y - 0.866_025).abs() < 1e-3, "{x},{y}");
        assert!(r["site"]["northArrow"]["at"].is_array());
    }
}
