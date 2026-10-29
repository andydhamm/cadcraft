//! Solar position calculations.
//!
//! Uses the NOAA fractional-year form for declination and hour angle.
//! Coordinate frame: +X east, +Y north, +Z up.
//! Azimuth is measured clockwise from North (0° = North, 90° = East).

/// The position of the sun.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sun {
    /// Elevation angle in degrees above the horizon.
    pub elevation_deg: f64,
    /// Azimuth angle in degrees clockwise from North.
    pub azimuth_deg: f64,
}

/// Calculate the day of the year for a given date string `YYYY-MM-DD`.
///
/// Returns `None` if the date is invalid or not in the expected format.
/// Leap years are handled correctly.
pub fn day_of_year(date: &str) -> Option<u32> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;

    if month < 1 || month > 12 {
        return None;
    }
    if day < 1 {
        return None;
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };

    if day > days_in_month {
        return None;
    }

    let mut doy = day;
    for m in 1..month {
        doy += match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if is_leap {
                    29
                } else {
                    28
                }
            }
            _ => return None,
        };
    }

    Some(doy)
}

/// Calculate the sun's position.
///
/// * `lat_deg`: Latitude in degrees (positive North).
/// * `date`: Date string `YYYY-MM-DD`.
/// * `time`: Time string `HH:MM` (local solar time) or `solar-noon`.
///
/// Returns `None` if inputs are invalid or non-finite.
pub fn sun_position(lat_deg: f64, date: &str, time: &str) -> Option<Sun> {
    if !lat_deg.is_finite() || lat_deg.abs() > 90.0 {
        return None;
    }

    let doy = day_of_year(date)?;

    let hour = if time == "solar-noon" {
        12.0
    } else {
        let parts: Vec<&str> = time.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let h: u32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        if h > 24 || m > 59 {
            return None;
        }
        h as f64 + (m as f64) / 60.0
    };

    if !hour.is_finite() {
        return None;
    }

    // NOAA fractional year form
    // g = 2 * pi / 365 * (doy - 1 + (hour - 12) / 24)
    let g = 2.0 * std::f64::consts::PI / 365.0 * ((doy as f64) - 1.0 + (hour - 12.0) / 24.0);

    // Declination in radians
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();

    // Hour angle in degrees
    let h_deg = 15.0 * (hour - 12.0);
    let h_rad = h_deg.to_radians();

    let lat_rad = lat_deg.to_radians();

    // sin(el) = sin(lat)sin(decl) + cos(lat)cos(decl)cos(H)
    let sin_el = lat_rad.sin() * decl.sin() + lat_rad.cos() * decl.cos() * h_rad.cos();

    // Clamp to [-1, 1] to avoid NaN from asin
    let sin_el_clamped = sin_el.clamp(-1.0, 1.0);
    let el_rad = sin_el_clamped.asin();
    let elevation_deg = el_rad.to_degrees();

    // Azimuth
    // az = atan2(sin H, cos H sin lat - tan decl cos lat) + 180
    // Note: The formula provided in the prompt uses atan2(sin H, ...).
    // Standard NOAA formula often uses atan2(sin H, cos H * sin lat - tan decl * cos lat)
    // which yields azimuth from South, then adds 180 to get from North?
    // Let's follow the prompt strictly:
    // az = atan2(sin H, cos H sin lat - tan decl cos lat) + 180deg
    let cos_h = h_rad.cos();
    let sin_h = h_rad.sin();
    let sin_lat = lat_rad.sin();
    let cos_lat = lat_rad.cos();
    let tan_decl = decl.tan();

    let y = sin_h;
    let x = cos_h * sin_lat - tan_decl * cos_lat;

    let mut az_rad = y.atan2(x) + std::f64::consts::PI;

    // Normalize to [0, 360)
    let mut az_deg = az_rad.to_degrees();
    az_deg = az_deg.rem_euclid(360.0);

    if !elevation_deg.is_finite() || !az_deg.is_finite() {
        return None;
    }

    Some(Sun { elevation_deg, azimuth_deg: az_deg })
}

/// Calculate the unit vector pointing towards the sun.
///
/// Frame: +X East, +Y North, +Z Up.
/// Returns `[0.0, 0.0, 1.0]` if input is invalid.
pub fn sun_dir(s: &Sun) -> [f64; 3] {
    if !s.elevation_deg.is_finite() || !s.azimuth_deg.is_finite() {
        return [0.0, 0.0, 1.0];
    }

    let el_rad = s.elevation_deg.to_radians();
    let az_rad = s.azimuth_deg.to_radians();

    let cos_el = el_rad.cos();
    let sin_el = el_rad.sin();
    let cos_az = az_rad.cos();
    let sin_az = az_rad.sin();

    let x = cos_el * sin_az;
    let y = cos_el * cos_az;
    let z = sin_el;

    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return [0.0, 0.0, 1.0];
    }

    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_noon_at_lat_51_05_on_the_summer_solstice() {
        // 51.05 lat, 2026-06-21, solar-noon
        // Expected: elevation 62.40 ± 0.05
        let sun = sun_position(51.05, "2026-06-21", "solar-noon").unwrap();
        assert!((sun.elevation_deg - 62.40).abs() < 0.05, "Elevation: {}", sun.elevation_deg);
    }

    #[test]
    fn solar_noon_azimuth_is_due_south() {
        // Azimuth should be 180 ± 0.1
        let sun = sun_position(51.05, "2026-06-21", "solar-noon").unwrap();
        assert!((sun.azimuth_deg - 180.0).abs() < 0.1, "Azimuth: {}", sun.azimuth_deg);
    }

    #[test]
    fn winter_solstice_is_much_lower() {
        // 2026-12-21, solar-noon
        // Expected: elevation ≈ 15.5 ± 0.5
        let sun = sun_position(51.05, "2026-12-21", "solar-noon").unwrap();
        assert!((sun.elevation_deg - 15.5).abs() < 0.5, "Elevation: {}", sun.elevation_deg);
        assert!((sun.azimuth_deg - 180.0).abs() < 0.1, "Azimuth: {}", sun.azimuth_deg);
    }

    #[test]
    fn morning_is_east_afternoon_is_west() {
        // 06:00 az < 120
        let sun = sun_position(51.05, "2026-06-21", "06:00").unwrap();
        assert!(sun.azimuth_deg < 120.0, "Morning Azimuth: {}", sun.azimuth_deg);

        // 18:00 az > 240 (West)
        let sun_pm = sun_position(51.05, "2026-06-21", "18:00").unwrap();
        assert!(sun_pm.azimuth_deg > 240.0, "Afternoon Azimuth: {}", sun_pm.azimuth_deg);
    }

    #[test]
    fn sun_dir_is_unit_and_up_when_above_the_horizon() {
        let sun = Sun {
            elevation_deg: 45.0,
            azimuth_deg: 90.0, // East
        };
        let dir = sun_dir(&sun);

        // Unit vector check
        let len = (dir[0].powi(2) + dir[1].powi(2) + dir[2].powi(2)).sqrt();
        assert!((len - 1.0).abs() < 1e-6, "Length: {}", len);

        // Up check
        assert!(dir[2] > 0.0, "Z should be positive");

        // East check (X positive, Y near 0)
        assert!(dir[0] > 0.0, "X should be positive for East");
        assert!(dir[1].abs() < 1e-6, "Y should be near 0 for East");
    }

    #[test]
    fn hostile_dates_and_times_are_rejected() {
        assert!(sun_position(51.05, "2026-13-01", "solar-noon").is_none());
        assert!(sun_position(51.05, "2026-02-30", "solar-noon").is_none());
        assert!(sun_position(51.05, "2026-06-21", "25:00").is_none());
        assert!(sun_position(51.05, "2026-06-21", "12:60").is_none());
        assert!(sun_position(91.0, "2026-06-21", "solar-noon").is_none());
        assert!(sun_position(f64::NAN, "2026-06-21", "solar-noon").is_none());
        assert!(sun_position(51.05, "invalid", "solar-noon").is_none());
    }

    #[test]
    fn day_of_year_is_leap_aware() {
        // 2024 is a leap year
        assert_eq!(day_of_year("2024-01-01"), Some(1));
        assert_eq!(day_of_year("2024-02-29"), Some(60));
        assert_eq!(day_of_year("2024-03-01"), Some(61));

        // 2023 is not a leap year
        assert_eq!(day_of_year("2023-01-01"), Some(1));
        assert_eq!(day_of_year("2023-02-28"), Some(59));
        assert_eq!(day_of_year("2023-03-01"), Some(60));
        assert_eq!(day_of_year("2023-02-29"), None);

        // Dec 31
        assert_eq!(day_of_year("2024-12-31"), Some(366));
        assert_eq!(day_of_year("2023-12-31"), Some(365));
    }

    #[test]
    fn equator_on_equinox_sun_is_overhead() {
        // At the equator on the spring equinox, the sun is directly overhead at solar noon.
        let sun = sun_position(0.0, "2026-03-20", "solar-noon").unwrap();
        assert!((sun.elevation_deg - 90.0).abs() < 0.1, "Elevation at equator on equinox should be ~90°, got {}", sun.elevation_deg);
    }

    #[test]
    fn north_pole_summer_solstice_sun_is_above_horizon() {
        // At the North Pole on the summer solstice, the sun is above the horizon 24 hours.
        let sun_noon = sun_position(90.0, "2026-06-21", "solar-noon").unwrap();
        assert!(sun_noon.elevation_deg > 0.0, "Noon elevation at North Pole on summer solstice should be > 0°, got {}", sun_noon.elevation_deg);

        let sun_midnight = sun_position(90.0, "2026-06-21", "00:00").unwrap();
        assert!(
            sun_midnight.elevation_deg > 0.0,
            "Midnight elevation at North Pole on summer solstice should be > 0°, got {}",
            sun_midnight.elevation_deg
        );
    }

    #[test]
    fn sun_is_below_horizon_at_night() {
        // At night, the sun should be below the horizon (negative elevation).
        let sun = sun_position(51.05, "2026-06-21", "00:00").unwrap();
        assert!(sun.elevation_deg < 0.0, "Elevation at midnight should be negative, got {}", sun.elevation_deg);
    }

    #[test]
    fn sun_dir_cardinal_directions() {
        // South (azimuth 180°)
        let sun_south = Sun { elevation_deg: 45.0, azimuth_deg: 180.0 };
        let dir_south = sun_dir(&sun_south);
        assert!((dir_south[0]).abs() < 1e-6, "X should be near 0 for South");
        assert!(dir_south[1] < 0.0, "Y should be negative for South");
        assert!(dir_south[2] > 0.0, "Z should be positive");

        // North (azimuth 0°)
        let sun_north = Sun { elevation_deg: 45.0, azimuth_deg: 0.0 };
        let dir_north = sun_dir(&sun_north);
        assert!((dir_north[0]).abs() < 1e-6, "X should be near 0 for North");
        assert!(dir_north[1] > 0.0, "Y should be positive for North");
        assert!(dir_north[2] > 0.0, "Z should be positive");

        // West (azimuth 270°)
        let sun_west = Sun { elevation_deg: 45.0, azimuth_deg: 270.0 };
        let dir_west = sun_dir(&sun_west);
        assert!(dir_west[0] < 0.0, "X should be negative for West");
        assert!((dir_west[1]).abs() < 1e-6, "Y should be near 0 for West");
        assert!(dir_west[2] > 0.0, "Z should be positive");
    }

    #[test]
    fn time_format_with_single_digit_hour() {
        // "0:00" should be accepted as midnight.
        let sun = sun_position(51.05, "2026-06-21", "0:00").unwrap();
        assert!(sun.elevation_deg < 0.0, "Midnight elevation should be negative, got {}", sun.elevation_deg);
    }

    #[test]
    fn century_leap_year_rules() {
        // Year 2000 is a leap year (divisible by 400).
        assert_eq!(day_of_year("2000-02-29"), Some(60));
        // Year 1900 is not a leap year (divisible by 100 but not 400).
        assert_eq!(day_of_year("1900-02-29"), None);
        // Year 2400 is a leap year (divisible by 400).
        assert_eq!(day_of_year("2400-02-29"), Some(60));
    }

    #[test]
    fn sun_dir_overhead_is_straight_up() {
        // When the sun is directly overhead (elevation 90°), direction should be [0, 0, 1].
        let sun = Sun { elevation_deg: 90.0, azimuth_deg: 0.0 };
        let dir = sun_dir(&sun);
        assert!((dir[0]).abs() < 1e-6, "X should be near 0 for overhead sun");
        assert!((dir[1]).abs() < 1e-6, "Y should be near 0 for overhead sun");
        assert!((dir[2] - 1.0).abs() < 1e-6, "Z should be 1 for overhead sun, got {}", dir[2]);
    }
}
