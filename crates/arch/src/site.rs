//! Site location (phase 9): true north, the north arrow, and turning true-north directions into
//! drawing directions.

use cadcraft_doc::arch::{ArchModel, SiteLocation};
use cadcraft_geom::Vec2;

/// The north arrow in the plan.
#[derive(Clone, Debug, PartialEq)]
pub struct NorthArrow {
    pub at: Vec2,
    /// Unit vector toward true north in drawing coordinates.
    pub direction: Vec2,
    /// Closed outline of the arrow head and shaft.
    pub outline: Vec<Vec2>,
}

/// Check a site location before it is stored.
pub fn validate_site(site: &SiteLocation) -> Result<(), String> {
    if !site.latitude.is_finite() || !(-90.0..=90.0).contains(&site.latitude) {
        return Err("latitude must be within -90..90".into());
    }
    if !site.longitude.is_finite() || !(-180.0..=180.0).contains(&site.longitude) {
        return Err("longitude must be within -180..180".into());
    }
    if !site.elevation.is_finite() || site.elevation.abs() > 10000.0 {
        return Err("elevation must be a finite number within 10000 m".into());
    }
    if !site.true_north.is_finite() {
        return Err("true_north must be a finite number".into());
    }
    if let Some(r) = &site.reference {
        if !r.point.x.is_finite() || !r.point.y.is_finite() {
            return Err("reference point must be finite".into());
        }
        if !r.easting.is_finite() {
            return Err("reference easting must be finite".into());
        }
        if !r.northing.is_finite() {
            return Err("reference northing must be finite".into());
        }
        if r.crs.chars().count() > 64 {
            return Err("reference crs must be at most 64 characters".into());
        }
    }
    if let Some(a) = site.north_arrow_at
        && (!a.x.is_finite() || !a.y.is_finite())
    {
        return Err("north_arrow_at must be finite".into());
    }
    Ok(())
}

/// Unit vector toward true north in drawing coordinates.
pub fn north_direction(site: &SiteLocation) -> Vec2 {
    let t = site.true_north.to_radians();
    Vec2::new(-t.sin(), t.cos())
}

/// Turn a vector given with +Y = true north (+X = east) into drawing coordinates.
pub fn true_to_drawing(site: Option<&SiteLocation>, v: [f64; 3]) -> [f64; 3] {
    let Some(site) = site else { return v };
    let (s, c) = site.true_north.to_radians().sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
}

/// The north arrow, when the drawing has a site location or a terrain.
pub fn north_arrow(model: &ArchModel) -> Option<NorthArrow> {
    if model.site.is_none() && model.terrain.is_none() {
        return None;
    }
    let direction = model.site.as_ref().map_or(Vec2::new(0.0, 1.0), north_direction);
    let at = match model.site.as_ref().and_then(|s| s.north_arrow_at) {
        Some(p) => p,
        None => match &model.terrain {
            Some(t) if !t.boundary.is_empty() => {
                let max_x = t.boundary.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
                let max_y = t.boundary.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
                Vec2::new(max_x + 3.0, max_y - 3.0)
            }
            _ => Vec2::new(0.0, 0.0),
        },
    };
    let perp = Vec2::new(-direction.y, direction.x);
    let base = Vec2::new(at.x + 1.4 * direction.x, at.y + 1.4 * direction.y);
    let tip = Vec2::new(at.x + 2.0 * direction.x, at.y + 2.0 * direction.y);
    let left = Vec2::new(base.x + 0.35 * perp.x, base.y + 0.35 * perp.y);
    let right = Vec2::new(base.x - 0.35 * perp.x, base.y - 0.35 * perp.y);
    let outline = vec![at, base, left, tip, right, base, at];
    Some(NorthArrow { at, direction, outline })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::arch::Terrain;

    fn site(north: f64) -> SiteLocation {
        SiteLocation { true_north: north, ..SiteLocation::default() }
    }

    #[test]
    fn north_direction_turns_counter_clockwise() {
        let d = north_direction(&site(30.0));
        assert!((d.x + 0.5).abs() < 1e-9 && (d.y - 0.866_025_4).abs() < 1e-6);
    }

    #[test]
    fn true_to_drawing_rotates_east_and_north() {
        let s = site(90.0);
        let n = true_to_drawing(Some(&s), [0.0, 1.0, 2.0]);
        assert!((n[0] + 1.0).abs() < 1e-9 && n[1].abs() < 1e-9 && n[2] == 2.0);
        let e = true_to_drawing(Some(&s), [1.0, 0.0, 0.0]);
        assert!(e[0].abs() < 1e-9 && (e[1] - 1.0).abs() < 1e-9);
        assert_eq!(true_to_drawing(None, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn north_arrow_sits_beside_the_terrain() {
        let mut m = ArchModel::default();
        assert!(north_arrow(&m).is_none());
        m.terrain = Some(Terrain { boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(20.0, 10.0)], ..Terrain::default() });
        let a = north_arrow(&m).expect("arrow");
        assert_eq!(a.at, Vec2::new(23.0, 7.0));
        assert_eq!(a.direction, Vec2::new(0.0, 1.0));
        assert_eq!(a.outline.first(), a.outline.last());
        assert!(a.outline.contains(&Vec2::new(23.0, 9.0)));
    }

    #[test]
    fn validate_site_refuses_bad_latitude_and_nan() {
        assert!(validate_site(&site(10.0)).is_ok());
        let mut s = site(0.0);
        s.latitude = 91.0;
        assert_eq!(validate_site(&s).unwrap_err(), "latitude must be within -90..90");
        let mut s = site(f64::NAN);
        assert!(validate_site(&s).is_err());
        s.true_north = 0.0;
        s.longitude = f64::NAN;
        assert!(validate_site(&s).is_err());
    }
}
