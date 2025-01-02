use pyo3::prelude::*;
use ::hfd::HFDownloader as RustHFDownloader;

#[pyclass(name = "HFDownloader")]
#[derive(Debug)]
struct PyHFDownloader {
    inner: RustHFDownloader,
}

#[pymethods]
impl PyHFDownloader {
    #[new]
    fn new(repo_id: &str, token: Option<String>, local_dir: Option<&str>) -> Self {
        Self {
            inner: RustHFDownloader::new(repo_id, token, local_dir),
        }
    }

    fn download(&self) -> PyResult<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(self.inner.download())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(())
    }
}

#[pymodule]
#[pyo3(name = "hfd")]
fn hfd(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyHFDownloader>()?;
    Ok(())
} 