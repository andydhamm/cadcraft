//! `sun` command: solar position for a latitude, date and time.

use serde_json::{Value, json};

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad, f64_req},
};

/// `sun` command spec.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("sun", "Sun", run_sun).menu(&["Architecture", "Sun"]).params("{lat,date,time}").noundo()]
}

/// Report the sun's elevation, azimuth and direction vector.
///
/// `lat` is a finite number in [-90, 90]; `date` is `YYYY-MM-DD`; `time` is `solar-noon` or
/// `HH:MM`. Anything missing or invalid is refused, never clamped. The document is not touched.
/// Returns `{"ok":true,"elevation":deg,"azimuth":deg,"dir":[x,y,z],"lat":..,"date":..,"time":..}`.
fn run_sun(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let lat = f64_req("sun", params, "lat")?;
    if lat.abs() > 90.0 {
        return Err(bad("sun", "`lat` must be within -90..90"));
    }
    let date = params.get("date").and_then(Value::as_str).ok_or_else(|| bad("sun", "`date` (string) is required"))?;
    let time = params.get("time").and_then(Value::as_str).ok_or_else(|| bad("sun", "`time` (solar-noon or HH:MM) is required"))?;
    let sun = cadcraft_arch::sun::sun_position(lat, date, time).ok_or_else(|| bad("sun", "invalid `date` or `time`"))?;
    let dir = cadcraft_arch::sun::sun_dir(&sun);
    Ok(json!({
        "ok": true,
        "elevation": sun.elevation_deg,
        "azimuth": sun.azimuth_deg,
        "dir": dir,
        "lat": lat,
        "date": date,
        "time": time,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_command_reports_solar_noon_at_lat_51_05() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 51.05, "date": "2024-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["ok"], json!(true));
        let el = r["elevation"].as_f64().unwrap();
        assert!((el - (90.0 - 51.05 + 23.44)).abs() < 1.0, "elevation {el}");
        let az = r["azimuth"].as_f64().unwrap();
        assert!((az - 180.0).abs() < 2.0, "azimuth {az}");
        let d: Vec<f64> = r["dir"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(d.len(), 3);
        assert!(d[1] < 0.0 && d[2] > 0.0);
        assert_eq!(r["time"], json!("solar-noon"));
    }

    #[test]
    fn sun_command_refuses_bad_date_time_and_latitude() {
        let mut s = Session::new();
        let bads = [
            json!({ "date": "2024-06-21", "time": "12:00" }),
            json!({ "lat": "x", "date": "2024-06-21", "time": "12:00" }),
            json!({ "lat": 91.0, "date": "2024-06-21", "time": "12:00" }),
            json!({ "lat": -90.5, "date": "2024-06-21", "time": "12:00" }),
            json!({ "lat": 10.0, "time": "12:00" }),
            json!({ "lat": 10.0, "date": 5, "time": "12:00" }),
            json!({ "lat": 10.0, "date": "2024-13-45", "time": "12:00" }),
            json!({ "lat": 10.0, "date": "nope", "time": "12:00" }),
            json!({ "lat": 10.0, "date": "2024-06-21" }),
            json!({ "lat": 10.0, "date": "2024-06-21", "time": "25:99" }),
            json!({ "lat": 10.0, "date": "2024-06-21", "time": "noonish" }),
        ];
        for p in &bads {
            assert!(s.execute("sun", p).is_err(), "should refuse {p}");
        }
    }

    #[test]
    fn sun_command_does_not_touch_the_document() {
        let mut s = Session::new();
        let before = s.doc().unwrap().arch_revision;
        let n = s.doc().unwrap().arch.walls.len();
        s.execute("sun", &json!({ "lat": 0.0, "date": "2024-03-20", "time": "09:30" })).unwrap();
        let doc = s.doc().unwrap();
        assert_eq!(doc.arch_revision, before);
        assert_eq!(doc.arch.walls.len(), n);
    }
}
