use crate::utils::GeoPoint;
use failure::Fail;
use std::fmt;
use std::num::ParseFloatError;
use std::str::FromStr;

/// Represent a list of points and can be parsed or expressed in the OSRM format:
/// {longitude},{latitude};{longitude},{latitude}[;{longitude},{latitude} ...]
#[derive(Clone, Debug, PartialEq)]
pub struct Coordinates(pub Vec<GeoPoint>);

/// Errors that can happen when parsin Coordinates from a String
#[derive(Debug, Fail, PartialEq)]
pub enum ParseCoordinatesError {
    #[fail(
        display = "Expected at least {} lon_lat pairs, got {}. Use ';' to separate pairs",
        expected, got
    )]
    NotEnoughLonLatPairs { got: usize, expected: usize },
    #[fail(
        display = "Missing latitude component in pair {}. Use ',' to separate longitude and latitude",
        pair
    )]
    MissingLat { pair: String },
    #[fail(
        display = "Too many values to parse in pair {}. Use ',' to separate longitude and latitude",
        pair
    )]
    ExtraValue { pair: String },
    #[fail(display = "Could not parse pair {}: {}", pair, source)]
    InvalidFloat {
        pair: String,
        #[cause]
        source: ParseFloatError,
    },
    #[fail(
        display = "Value {} in pair {} is out of range, it should in [{}, {}]. Make sure to use the order longitude,latitude",
        got, pair, expected_min, expected_max
    )]
    InvalidRange {
        pair: String,
        got: f64,
        expected_min: f64,
        expected_max: f64,
    },
}

// String -> Coordinates
impl FromStr for Coordinates {
    type Err = ParseCoordinatesError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// Parse a float and also check its bounds
        fn parse_and_check_float(
            pair: &str,
            v: &str,
            min: f64,
            max: f64,
        ) -> Result<f64, ParseCoordinatesError> {
            let v: f64 = v
                .parse()
                .map_err(|source| ParseCoordinatesError::InvalidFloat {
                    pair: pair.to_owned(),
                    source,
                })?;

            if v < min || v > max {
                return Err(ParseCoordinatesError::InvalidRange {
                    pair: pair.to_owned(),
                    got: v,
                    expected_min: min,
                    expected_max: max,
                });
            }

            Ok(v)
        }

        let mut points: Vec<GeoPoint> = Vec::new();

        for pair in s.split(';') {
            // Split pairs in ','
            let lon_lat: Vec<&str> = pair.split(',').collect();
            if lon_lat.len() < 2 {
                return Err(ParseCoordinatesError::MissingLat {
                    pair: pair.to_owned(),
                });
            } else if lon_lat.len() > 2 {
                return Err(ParseCoordinatesError::ExtraValue {
                    pair: pair.to_owned(),
                });
            }

            let lon = parse_and_check_float(pair, lon_lat[0], -180., 180.)?;
            let lat = parse_and_check_float(pair, lon_lat[1], -90., 90.)?;

            points.push(GeoPoint::from_degrees(lat, lon));
        }

        if points.len() < 2 {
            return Err(ParseCoordinatesError::NotEnoughLonLatPairs {
                got: points.len(),
                expected: 2,
            });
        }

        Ok(Coordinates(points))
    }
}

// Coordinates -> String
impl fmt::Display for Coordinates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, point) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ";")?;
            }

            write!(f, "{},{}", point.lon.as_degrees(), point.lat.as_degrees())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn coordinates() {
        // Parse back and forth
        let s = "13.38886,52.517037;13.397634,52.529407;13.428555,52.523219";
        let c: Coordinates = s.parse().unwrap();
        assert_eq!(c.to_string(), s);

        // Parse errors
        fn check_failed_parse(s: &str, fail: &str) {
            assert_eq!(s.parse::<Coordinates>().unwrap_err().to_string(), fail);
        }
        check_failed_parse(
            "13.38886,52.517037",
            "Expected at least 2 lon_lat pairs, got 1. Use ';' to separate pairs",
        );
        check_failed_parse("13.38886;13.397634,52.529407;13.428555,52.523219", "Missing latitude component in pair 13.38886. Use ',' to separate longitude and latitude");
        check_failed_parse(
            "13.38886,52.517037,17;13.397634,52.529407;13.428555,52.523219",
            "Too many values to parse in pair 13.38886,52.517037,17. Use ',' to separate longitude and latitude",
        );
        check_failed_parse(
            "13.38886,banana;13.397634,52.529407;13.428555,52.523219",
            "Could not parse pair 13.38886,banana: invalid float literal",
        );
        check_failed_parse(
            "1300.38886,52.517037;13.397634,52.529407;13.428555,52.523219",
            "Value 1300.38886 in pair 1300.38886,52.517037 is out of range, it should in [-180, 180]. Make sure to use the order longitude,latitude",
        );
        check_failed_parse(
            "13.38886,5200.517037;13.397634,52.529407;13.428555,52.523219",
            "Value 5200.517037 in pair 13.38886,5200.517037 is out of range, it should in [-90, 90]. Make sure to use the order longitude,latitude",
        );
    }
}
