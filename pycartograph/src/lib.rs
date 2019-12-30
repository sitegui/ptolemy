use cartograph::Cartography as InnerCartography;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Create a cartography struct by reading the Ptolemy file
#[pyclass]
#[text_signature = "(file_path, /)"]
struct Cartography {
    inner: InnerCartography,
}

#[pymethods]
impl Cartography {
    #[new]
    fn new(obj: &PyRawObject, file_path: String) -> PyResult<()> {
        let inner = InnerCartography::open(file_path)?;
        Ok(obj.init(Cartography { inner }))
    }

    /// Returns a sample of the edges inside a given region, described by two opposite corners in x, y coordinates.
    /// This function can return less than `max_num` even when there are more than that, please refer to the
    ///  PrioritySample trait to understand how sampling works
    #[text_signature = "(xy1, xy2, max_num, /)"]
    pub fn sample_edges(
        &self,
        py: Python,
        xy1: (f32, f32),
        xy2: (f32, f32),
        max_num: usize,
    ) -> PyResult<PyObject> {
        // Get edges
        let edges_by_level = self.inner.sample_edges(xy1, xy2, max_num);

        // Transform each level into a HoloViews Path dict
        let result = PyDict::new(py);
        for (level, edges) in edges_by_level {
            // Collect x and y, interleaving with NaN between lines
            let x = PyList::empty(py);
            let y = PyList::empty(py);
            for edge_index in edges {
                let (_edge, source, target) = self.inner.edge_info(edge_index);
                x.append(source.x)?;
                x.append(target.x)?;
                x.append(std::f32::NAN)?;
                y.append(source.y)?;
                y.append(target.y)?;
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
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn cartograph(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Cartography>()?;

    Ok(())
}
