use log::{debug, trace, info};
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use pyo3_log::{Caching, Logger};

#[pyfunction]
#[text_signature = "()"]
fn log_hello() {
    trace!("xyz");
    debug!("stuff2");
    debug!("Stuff");
    info!("Hello {}", "world");
    info!("Hello 2{}", "world");
}

#[pymodule]
fn hello_world(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    let _ = Logger::new(py, Caching::LoggersAndLevels)?
        .install();

    m.add_wrapped(wrap_pyfunction!(log_hello))?;

    Ok(())
}
