use log::{debug, info, trace};
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3_log::{Caching, Logger};

#[pyfunction]
fn log_hello() {
    trace!("xyz");
    debug!("stuff2");
    debug!("Stuff");
    info!("Hello {}", "world");
    info!("Hello 2{}", "world");
    info!(test = 5; "Hello world with KV");
}

#[pymodule]
fn hello_world(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = Logger::new(py, Caching::LoggersAndLevels)?
        .set_prefix("test_prefix")
        .install();

    m.add_wrapped(wrap_pyfunction!(log_hello))?;

    Ok(())
}
