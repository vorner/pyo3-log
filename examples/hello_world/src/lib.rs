use log::{debug, trace, info};
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3_log::{Caching, Logger};

#[pyfunction]
#[pyo3(signature = (prefix=None))]
fn enable_logging(py: Python<'_>, prefix: Option<String>) -> PyResult<()> {
    let mut logger = Logger::new(py, Caching::LoggersAndLevels)?;
    logger = if let Some(prefix) = prefix {
        logger.prefix(&prefix)
    } else {
        logger
    };

    let _ = logger.install();
    Ok(())
}

#[pyfunction]
fn log_hello() {
    trace!("xyz");
    debug!("stuff2");
    debug!("Stuff");
    info!("Hello {}", "world");
    info!("Hello 2{}", "world");
}

#[pymodule]
fn hello_world(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_wrapped(wrap_pyfunction!(enable_logging))?;
    m.add_wrapped(wrap_pyfunction!(log_hello))?;

    Ok(())
}
