//! `sun` command: solar position for a latitude, date and time.

use serde_json::{Value, json};

use crate::{
    EngineError, Session,
    cmd::{CommandSpec, bad, f64_req},
};

/// `sun` command spec.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("sun", "Sun", run_sun).menu(&["Architecture", "Sun"]).params("{lat?, date, time}").noundo()]
}

/// Report the sun's elevation, azimuth and direction vector.
///
/// `lat` is a finite number in [-90, 90]; `date` is `YYYY-MM-DD`; `time` is `solar-noon` or
/// `HH:MM`. Anything missing or invalid is refused, never clamped. The document is not touched.
/// Returns `{"ok":true,"elevation":deg,"azimuth":deg,"dir":[x,y,z],"lat":..,"date":..,"time":..}`.
fn run_sun(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let site = s.doc()?.arch.site.clone();
    let lat = match (params.get("lat"), &site) {
        (None | Some(Value::Null), Some(site)) => site.latitude,
        (None | Some(Value::Null), None) => return Err(bad("sun", "`lat` is required when the drawing has no site location")),
        _ => f64_req("sun", params, "lat")?,
    };
    if lat.abs() > 90.0 {
        return Err(bad("sun", "`lat` must be within -90..90"));
    }
    let date = params.get("date").and_then(Value::as_str).ok_or_else(|| bad("sun", "`date` (string) is required"))?;
    let time = params.get("time").and_then(Value::as_str).ok_or_else(|| bad("sun", "`time` (solar-noon or HH:MM) is required"))?;
    let sun = cadcraft_arch::sun::sun_position(lat, date, time).ok_or_else(|| bad("sun", "invalid `date` or `time`"))?;
    let dir = cadcraft_arch::site::true_to_drawing(site.as_ref(), cadcraft_arch::sun::sun_dir(&sun));
    let true_north = site.as_ref().map_or(0.0, |x| x.true_north);
    Ok(json!({
        "trueNorth": true_north,
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
    fn sun_uses_the_site_latitude_and_true_north() {
        let mut s = Session::new();
        s.execute("site", &json!({ "latitude": 51.05, "trueNorth": 30.0 })).unwrap();
        let r = s.execute("sun", &json!({ "date": "2026-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["trueNorth"], json!(30.0));
        let d: Vec<f64> = r["dir"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let (sx, sy) = (-d[0] / n, -d[1] / n);
        assert!((sx + 0.5).abs() < 1e-3 && (sy - 0.866).abs() < 1e-3, "shadow {sx},{sy}");
        // An explicit lat still wins.
        let r = s.execute("sun", &json!({ "lat": 0.0, "date": "2026-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["lat"], json!(0.0));
    }

    #[test]
    fn sun_without_site_or_lat_is_refused() {
        let mut s = Session::new();
        assert!(s.execute("sun", &json!({ "date": "2026-06-21", "time": "solar-noon" })).is_err());
        let r = s.execute("sun", &json!({ "lat": 10.0, "date": "2026-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["trueNorth"], json!(0.0));
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

    #[test]
    fn sun_command_accepts_boundary_latitudes() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 90.0, "date": "2024-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["ok"], json!(true), "lat=90 should be accepted");
        let r = s.execute("sun", &json!({ "lat": -90.0, "date": "2024-06-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["ok"], json!(true), "lat=-90 should be accepted");
    }

    #[test]
    fn sun_command_refuses_nan_latitude() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": f64::NAN, "date": "2024-06-21", "time": "12:00" }));
        assert!(r.is_err(), "NaN latitude should be refused");
    }

    #[test]
    fn sun_command_winter_solstice_elevation() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 51.05, "date": "2024-12-21", "time": "solar-noon" })).unwrap();
        assert_eq!(r["ok"], json!(true));
        let el = r["elevation"].as_f64().unwrap();
        // Winter solstice: elevation = 90 - lat - 23.44 = 15.51
        assert!((el - 15.5).abs() < 1.0, "winter solstice elevation {el}");
    }

    #[test]
    fn sun_command_dir_is_unit_vector() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 51.05, "date": "2024-06-21", "time": "solar-noon" })).unwrap();
        let d: Vec<f64> = r["dir"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-6, "dir should be unit vector, got len {len}");
    }

    #[test]
    fn sun_command_leap_year_date() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 0.0, "date": "2024-02-29", "time": "solar-noon" }));
        assert!(r.is_ok(), "2024-02-29 is a valid leap year date");
        let r = s.execute("sun", &json!({ "lat": 0.0, "date": "2023-02-29", "time": "solar-noon" }));
        assert!(r.is_err(), "2023-02-29 is not a valid date");
    }

    #[test]
    fn sun_command_echoes_input() {
        let mut s = Session::new();
        let r = s.execute("sun", &json!({ "lat": 45.5, "date": "2024-07-15", "time": "14:30" })).unwrap();
        assert_eq!(r["lat"], json!(45.5));
        assert_eq!(r["date"], json!("2024-07-15"));
        assert_eq!(r["time"], json!("14:30"));
    }
}
