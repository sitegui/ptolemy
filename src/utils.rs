use std::time::Instant;

/// Simple log helper that prepends messages with the elapsed time
pub struct DebugTime {
    start: Instant,
}

impl DebugTime {
    pub fn new() -> Self {
        DebugTime {
            start: Instant::now(),
        }
    }

    pub fn msg<T: std::fmt::Display>(&mut self, s: T) {
        let dt = Instant::now() - self.start;
        println!("[{:6.1}s] {}", dt.as_secs_f32(), s);
    }
}

/// Pretty format a number of bytes
pub fn format_bytes(n: u64) -> String {
    if n < 1000 {
        format!("{}B", n)
    } else if n < 1000 * 1024 {
        format!("{:.1}kiB", n as f32 / 1024.)
    } else if n < 1000 * 1024 * 1024 {
        format!("{:.1}MiB", n as f32 / 1024. / 1024.)
    } else {
        format!("{:.1}GiB", n as f32 / 1024. / 1024. / 1024.)
    }
}

/// Pretty format a number
pub fn format_num(n: usize) -> String {
    if n < 1000 {
        format!("{}", n)
    } else if n < 1000 * 1000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        format!("{:.1}M", n as f32 / 1000. / 1000.)
    }
}

/// Represent an angle in degrees with 1e-6 precision
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Angle(i32);

impl Angle {
    pub fn from_micro_degrees(a: i32) -> Self {
        Self(a)
    }
    pub fn from_degrees(a: f64) -> Self {
        Self((a * 1e6).round() as i32)
    }

    pub fn as_degrees(&self) -> f64 {
        self.0 as f64 / 1e6
    }

    pub fn as_radians(&self) -> f64 {
        self.as_degrees().to_radians()
    }

    pub fn as_micro_degrees(&self) -> i32 {
        self.0
    }
}

/// Represent a point on the surfase of the Earth (using the referential WGS84)
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct GeoPoint {
    pub lat: Angle,
    pub lon: Angle,
}

impl GeoPoint {
    /// Build a geo point from coordinate angles in degrees
    pub fn from_degrees(lat: f64, lon: f64) -> Self {
        Self {
            lat: Angle::from_degrees(lat),
            lon: Angle::from_degrees(lon),
        }
    }

    /// Build a geo point from coordinate angles in micro degrees
    pub fn from_micro_degrees(lat: i32, lon: i32) -> Self {
        Self {
            lat: Angle::from_micro_degrees(lat),
            lon: Angle::from_micro_degrees(lon),
        }
    }

    /// Return the projection of the point using Web Mercator coordinates
    /// (meters East of Greenwich and meters North of the Equator).
    pub fn web_mercator_project(&self) -> [f64; 2] {
        let a = 6_378_137.;
        let pi = std::f64::consts::PI;
        let lat_rad = self.lat.as_radians();
        let lon_rad = self.lon.as_radians();
        let easting = a * lon_rad;
        let northing = a * (pi / 4. + lat_rad / 2.).tan().ln();
        [easting, northing]
    }

    /// Get the Haversine distance in meters between this point and another one
    pub fn haversine_distance(&self, other: &GeoPoint) -> f64 {
        // Based on https://en.wikipedia.org/wiki/Haversine_formula and
        // https://github.com/georust/geo/blob/de873f9ec74ffb08d27d78be689a4a9e0891879f/geo/src/algorithm/haversine_distance.rs#L42-L52
        let theta1 = self.lat.as_radians();
        let theta2 = other.lat.as_radians();
        let delta_theta = other.lat.as_radians() - self.lat.as_radians();
        let delta_lambda = other.lon.as_radians() - self.lon.as_radians();
        let a = (delta_theta / 2.).sin().powi(2)
            + theta1.cos() * theta2.cos() * (delta_lambda / 2.).sin().powi(2);
        let c = 2. * a.sqrt().asin();
        6_371_000.0 * c
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn angle() {
        // Micro-degree precision
        assert_eq!(Angle::from_degrees(90.).as_degrees(), 90.);
        assert_eq!(Angle::from_degrees(90.000001).as_degrees(), 90.000001);
        assert_eq!(Angle::from_degrees(90.0000001).as_degrees(), 90.);

        assert_eq!(Angle::from_degrees(-90.).as_degrees(), -90.);
        assert_eq!(Angle::from_degrees(-90.000001).as_degrees(), -90.000001);
        assert_eq!(Angle::from_degrees(-90.0000001).as_degrees(), -90.);

        assert_eq!(
            Angle::from_degrees(90.).as_radians(),
            std::f64::consts::FRAC_PI_2
        );
        assert_eq!(Angle::from_degrees(90.).as_micro_degrees(), 90_000_000);
    }

    fn assert_f64_similar(left: f64, right: f64, max_error: f64) {
        assert!((left - right).abs() < max_error, "{} ~ {}", left, right)
    }

    #[test]
    fn point() {
        let zero = GeoPoint::from_degrees(0., 0.);
        let a = GeoPoint::from_degrees(36.12, -86.67);
        let b = GeoPoint::from_degrees(33.94, -118.4);

        assert_eq!(a.haversine_distance(&a).round(), 0.);
        assert_eq!(a.haversine_distance(&b).round(), 2886444.);

        let [x, y] = zero.web_mercator_project();
        assert_f64_similar(x, 0., 1e-6);
        assert_f64_similar(y, 0., 1e-6);

        let [x, y] = a.web_mercator_project();
        assert_f64_similar(x, -9648060.27, 1e-2);
        assert_f64_similar(y, 4317145.77, 1e-2);
    }
}
