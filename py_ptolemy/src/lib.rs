use ptolemy::Cartograph as InnerCartograph;
use ptolemy::GeoPoint;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Create a cartography struct by reading the Ptolemy file
#[pyclass]
#[text_signature = "(file_path, /)"]
struct Cartograph {
    inner: InnerCartograph,
}

#[pymethods]
impl Cartograph {
    #[new]
    fn new(obj: &PyRawObject, file_path: String) -> PyResult<()> {
        let inner = InnerCartograph::open(file_path)?;
        Ok(obj.init(Cartograph { inner }))
    }

    /// Convert a (lat, lon) point to (x, y) coordinates, used by Geoviews
    #[staticmethod]
    #[text_signature = "(latlon, /)"]
    pub fn web_mercator(latlon: (f64, f64)) -> (f64, f64) {
        let xy = GeoPoint::from_degrees(latlon.0, latlon.1).web_mercator_project();
        (xy[0], xy[1])
    }

    /// Convert from (x, y) coordinates, as used by Geoviews, to (lat, lon).
    /// Note that some methods in this class use the (lat, lon) format, so do not forget to
    /// use this when interfacing with Geoviews.
    /// As a general rule, the ones that finish with `_wm` use the Web Mercator (this is the case for some methods that
    /// were designed to be used mostly with Geoviews)
    #[staticmethod]
    #[text_signature = "(xy, /)"]
    pub fn inverse_web_mercator(xy: (f64, f64)) -> (f64, f64) {
        let p = GeoPoint::from_web_mercator([xy.0, xy.1]);
        (p.lat.as_degrees(), p.lon.as_degrees())
    }

    /// Returns a sample of the edges inside a given region, described by two opposite corners in (lat, lon) coordinates.
    /// This function can return less than `max_num` even when there are more than that, please refer to the
    ///  PrioritySample trait to understand how sampling works
    #[text_signature = "(xy1, xy2, max_num, /)"]
    pub fn sample_edges_wm(
        &self,
        py: Python,
        xy1: (f64, f64),
        xy2: (f64, f64),
        max_num: usize,
    ) -> PyResult<PyObject> {
        // Get edges
        let edges_by_level = self
            .inner
            .sample_edges([xy1.0, xy1.1], [xy2.0, xy2.1], max_num);

        // Transform each level into a HoloViews Path dict
        let result = PyDict::new(py);
        for (level, edges) in edges_by_level {
            // Collect x and y, interleaving with NaN between lines
            let x = PyList::empty(py);
            let y = PyList::empty(py);
            for edge_index in edges {
                let (_edge, source, target) = self.inner.edge_info(edge_index);
                let source = source.web_mercator_project();
                let target = target.web_mercator_project();
                x.append(source[0])?;
                x.append(target[0])?;
                x.append(std::f32::NAN)?;
                y.append(source[1])?;
                y.append(target[1])?;
                y.append(std::f32::NAN)?;
            }

            let dict = PyDict::new(py);
            dict.set_item("x", x)?;
            dict.set_item("y", y)?;
            result.set_item(level, dict)?;
        }

        Ok(result.to_object(py))
    }

    /// Compute the strongly connected components
    #[text_signature = "()"]
    pub fn strongly_connected_components(&self) -> Vec<Vec<u32>> {
        self.inner
            .strongly_connected_components()
            .into_iter()
            .map(|indexes| indexes.into_iter().map(|i| i.index() as u32).collect())
            .collect()
    }

    /// Compute the shortest path between two points, expressed in (lat, lon)
    #[text_signature = "(from, to, /)"]
    pub fn shortest_path(&self, from: (f64, f64), to: (f64, f64)) -> RoutePath {
        // Project nodes
        let from = self.inner.project(&GeoPoint::from_degrees(from.0, from.1));
        let to = self.inner.project(&GeoPoint::from_degrees(to.0, to.1));

        let path = self.inner.shortest_path(&from, &to);
        RoutePath {
            distance: path.distance,
            geometry: path.polyline,
        }
    }
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn ptolemy(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Cartograph>()?;

    Ok(())
}

/// Represent the a route path found
#[pyclass]
#[derive(Debug)]
struct RoutePath {
    /// The route distance in meters
    #[pyo3(get)]
    pub distance: u32,
    /// The shape of the route, encoded as a polyline
    #[pyo3(get)]
    pub geometry: String,
}
