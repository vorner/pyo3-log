#![forbid(unsafe_code)]

use std::cmp;
use std::collections::HashMap;

pub use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use pyo3::prelude::*;

// TODO: Get rid of…
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Caching {
    Nothing,
    LoggersAndLevels,
}

impl Default for Caching {
    fn default() -> Self {
        Caching::LoggersAndLevels
    }
}

// TODO: Some filtering... we don't want to call python & lock GIL with every damn trace message
#[derive(Clone, Debug)]
pub struct Logger {
    top_filter: LevelFilter,
    filters: HashMap<String, LevelFilter>,
    logging: Py<PyModule>,
    caching: Caching,
}

impl Logger {
    pub fn new(py: Python<'_>, caching: Caching) -> PyResult<Self> {
        let logging = py.import("logging")?;
        Ok(Self {
            top_filter: LevelFilter::Debug,
            filters: HashMap::new(),
            logging: logging.into(),
            caching,
        })
    }

    pub fn install(self) -> Result<(), SetLoggerError> {
        let level = cmp::max(
            self.top_filter,
            self.filters
                .values()
                .copied()
                .max()
                .unwrap_or(LevelFilter::Off),
        );
        log::set_max_level(level);
        log::set_boxed_logger(Box::new(self))
    }

    pub fn filter(mut self, filter: LevelFilter) -> Self {
        self.top_filter = filter;
        self
    }

    pub fn filter_target(mut self, target: String, filter: LevelFilter) -> Self {
        self.filters.insert(target, filter);
        self
    }

    fn get_logger<'r, 's: 'r, 'py: 'r>(&'s self, py: Python<'py>, target: &str) -> PyResult<&'r PyAny> {
        // TODO: Caching + somehow not requiring GIL in case we know from cache it doesn't exist
        let logging = self.logging.as_ref(py);
        let logger = logging.call1("getLogger", (target,))?;
        Ok(logger)
    }

    fn inner(&self, py: Python<'_>, record: &Record) -> PyResult<()> {
        let msg = format!("{}", record.args());
        let log_level = match record.level() {
            Level::Error => 40,
            Level::Warn => 30,
            Level::Info => 20,
            Level::Debug => 10,
            Level::Trace => 0,
        };
        let target = record.target().replace("::", ".");
        let logging = self.logging.as_ref(py);
        let logger = self.get_logger(py, &target)?;
        let none = py.None();
        let record = logging.call1(
            "LogRecord",
            (
                target,
                log_level,
                record.file(),
                record.line().unwrap_or_default(),
                msg,
                &none,
                &none,
            ),
        )?;
        // TODO: kv pairs, if enabled as a feature?
        logger.call_method1("handle", (record,))?;
        Ok(())
    }

    fn filter_for(&self, target: &str) -> LevelFilter {
        let mut start = 0;
        let mut filter = self.top_filter;
        while let Some(end) = target[start..].find("::") {
            if let Some(f) = self.filters.get(&target[..start + end]) {
                filter = *f;
            }
            start += end + 2;
        }
        if let Some(f) = self.filters.get(target) {
            filter = *f;
        }

        filter
    }
}

impl Default for Logger {
    fn default() -> Self {
        let gil = Python::acquire_gil();
        let py = gil.python();

        Self::new(py, Caching::LoggersAndLevels).expect("Failed to initialize python logging")
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // TODO: Consult the caches too once we have them (in conservative way, don't call into
        // python)

        metadata.level() <= self.filter_for(metadata.target())
    }

    fn log(&self, record: &Record) {
        // TODO: Caching of the loggers, modules, etc
        // TODO: Use the target to get the right logger
        if self.enabled(record.metadata()) {
            let gil = Python::acquire_gil();
            let py = gil.python();
            if let Err(e) = self.inner(py, record) {
                e.print(py);
            }
        }
    }

    fn flush(&self) {}
}

pub fn try_init() -> Result<(), SetLoggerError> {
    Logger::default().install()
}

pub fn init() {
    let _ = try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter() {
        let logger = Logger::default();
        assert_eq!(logger.filter_for("hello_world"), LevelFilter::Debug);
        assert_eq!(logger.filter_for("hello_world::sub"), LevelFilter::Debug);
    }

    #[test]
    fn set_filter() {
        let logger = Logger::default().filter(LevelFilter::Info);
        assert_eq!(logger.filter_for("hello_world"), LevelFilter::Info);
        assert_eq!(logger.filter_for("hello_world::sub"), LevelFilter::Info);
    }

    #[test]
    fn filter_specific() {
        let logger = Logger::default()
            .filter(LevelFilter::Warn)
            .filter_target("hello_world".to_owned(), LevelFilter::Debug)
            .filter_target("hello_world::sub".to_owned(), LevelFilter::Trace);
        assert_eq!(logger.filter_for("hello_world"), LevelFilter::Debug);
        assert_eq!(logger.filter_for("hello_world::sub"), LevelFilter::Trace);
        assert_eq!(logger.filter_for("hello_world::sub::multi::level"), LevelFilter::Trace);
        assert_eq!(logger.filter_for("hello_world::another"), LevelFilter::Debug);
        assert_eq!(logger.filter_for("hello_world::another::level"), LevelFilter::Debug);
        assert_eq!(logger.filter_for("other"), LevelFilter::Warn);
    }
}
