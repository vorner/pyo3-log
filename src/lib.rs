#![forbid(unsafe_code)]

use std::cmp;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use pyo3::prelude::*;

#[derive(Clone, Debug)]
pub struct ResetHandle(Arc<AtomicBool>);

impl ResetHandle {
    pub fn reset(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Caching {
    Nothing,
    Loggers,
    LoggersAndLevels,
}

impl Default for Caching {
    fn default() -> Self {
        Caching::LoggersAndLevels
    }
}

#[derive(Clone, Debug)]
struct CacheEntry {
    filter: LevelFilter,
    logger: PyObject,
}

#[derive(Clone, Debug, Default)]
struct CacheNode {
    local: Option<CacheEntry>,
    children: HashMap<String, Arc<CacheNode>>,
}

impl CacheNode {
    fn store_to_cache_recursive<'a, P>(&self, mut path: P, entry: CacheEntry) -> Arc<Self>
    where
        P: Iterator<Item = &'a str>,
    {
        let mut me = self.clone();
        match path.next() {
            Some(segment) => {
                let child = me.children.entry(segment.to_owned()).or_default();
                *child = child.store_to_cache_recursive(path, entry);
            }
            None => me.local = Some(entry),
        }
        Arc::new(me)
    }
}

#[derive(Clone, Debug)]
pub struct Logger {
    top_filter: LevelFilter,
    filters: HashMap<String, LevelFilter>,
    logging: Py<PyModule>,
    caching: Caching,
    cache: ArcSwap<CacheNode>,
    reset: Arc<AtomicBool>,
}

impl Logger {
    pub fn new(py: Python<'_>, caching: Caching) -> PyResult<Self> {
        let logging = py.import("logging")?;
        Ok(Self {
            top_filter: LevelFilter::Debug,
            filters: HashMap::new(),
            logging: logging.into(),
            caching,
            cache: Default::default(),
            reset: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn install(self) -> Result<ResetHandle, SetLoggerError> {
        let handle = self.reset_handle();
        let level = cmp::max(
            self.top_filter,
            self.filters
                .values()
                .copied()
                .max()
                .unwrap_or(LevelFilter::Off),
        );
        log::set_max_level(level);
        log::set_boxed_logger(Box::new(self))?;
        Ok(handle)
    }

    pub fn reset_handle(&self) -> ResetHandle {
        ResetHandle(Arc::clone(&self.reset))
    }

    pub fn filter(mut self, filter: LevelFilter) -> Self {
        self.top_filter = filter;
        self
    }

    pub fn filter_target(mut self, target: String, filter: LevelFilter) -> Self {
        self.filters.insert(target, filter);
        self
    }

    fn lookup(&self, target: &str) -> Option<Arc<CacheNode>> {
        if self.reset.load(Ordering::Relaxed) {
            self.cache.store(Default::default());
            self.reset.compare_and_swap(false, true, Ordering::SeqCst);
        }

        if self.caching == Caching::Nothing {
            return None;
        }

        let root = self.cache.load();
        let mut node: &Arc<CacheNode> = &root;
        for segment in target.split("::") {
            match node.children.get(segment) {
                Some(sub) => node = sub,
                None => return None,
            }
        }

        Some(Arc::clone(node))
    }

    /// Logs stuff
    ///
    /// Returns a logger to be cached, if any. If it already found a cached logger or if caching is
    /// turned off, returns None.
    fn log_inner(&self, py: Python<'_>, record: &Record, cache: &Option<Arc<CacheNode>>)
        -> PyResult<Option<PyObject>>
    {
        let msg = format!("{}", record.args());
        let log_level = map_level(record.level());
        let target = record.target().replace("::", ".");
        let cached_logger = cache
            .as_ref()
            .and_then(|node| node.local.as_ref())
            .map(|local| &local.logger);
        let (logger, cached) = match cached_logger {
            Some(cached) => (cached.as_ref(py), true),
            None => (self.logging.as_ref(py).call1("getLogger", (&target,))?, false),
        };
        dbg!((logger, cached));
        // We need to check for this ourselves. For some reason, the logger.handle does not check
        // it. And besides, we can save ourselves few python calls if it's turned off.
        if is_enabled_for(logger, record.level())? {
            let none = py.None();
            // TODO: kv pairs, if enabled as a feature?
            let record = logger.call_method1(
                "makeRecord",
                (
                    target,
                    log_level,
                    record.file(),
                    record.line().unwrap_or_default(),
                    msg,
                    &none, // args
                    &none, // exc_info
                ),
            )?;
            logger.call_method1("handle", (record,))?;
        }

        let cache_logger = if !cached && self.caching != Caching::Nothing {
            Some(logger.into())
        } else {
            None
        };

        Ok(cache_logger)
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

    fn enabled_inner(&self, metadata: &Metadata, cache: &Option<Arc<CacheNode>>) -> bool {
        let cache_filter = cache.as_ref()
            .and_then(|node| node.local.as_ref())
            .map(|local| local.filter)
            .unwrap_or_else(LevelFilter::max);

        metadata.level() <= cache_filter && metadata.level() <= self.filter_for(metadata.target())
    }

    fn store_to_cache(&self, target: &str, entry: CacheEntry) {
        let path = target.split("::");

        let orig = self.cache.load();
        // Construct a new cache structure and insert the new root.
        let new = orig.store_to_cache_recursive(path, entry);
        // Note: In case of collision, the cache update is lost. This is fine, as we simply lose a
        // tiny bit of performance and will cache the thing next time.
        //
        // We err on the side of losing it here (instead of overwriting), because if the cache is
        // reset, we don't want to re-insert the old value we have.
        self.cache.compare_and_swap(orig, new);
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
        let cache = self.lookup(metadata.target());

        self.enabled_inner(metadata, &cache)
    }

    fn log(&self, record: &Record) {
        let cache = self.lookup(record.target());

        let mut store_to_cache = None;
        if self.enabled_inner(record.metadata(), &cache) {
            let gil = Python::acquire_gil();
            let py = gil.python();
            match self.log_inner(py, record, &cache) {
                Ok(Some(logger)) => {
                    let filter = match self.caching {
                        Caching::Nothing => unreachable!(),
                        Caching::Loggers => LevelFilter::max(),
                        Caching::LoggersAndLevels => extract_max_level(py, &logger)
                            .unwrap_or_else(|e| {
                                e.print(py);
                                LevelFilter::max()
                            })
                    };
                    store_to_cache = Some((logger, filter));
                },
                Ok(None) => (),
                Err(e) => e.print(py),
            }
        }
        // Note: no more GIL here. Not needed for storing to cache.

        if let Some((logger, filter)) = store_to_cache {
            let entry = CacheEntry {
                logger,
                filter,
            };
            self.store_to_cache(record.target(), entry);
        }
    }

    fn flush(&self) {}
}

fn map_level(level: Level) -> usize {
    match level {
        Level::Error => 40,
        Level::Warn => 30,
        Level::Info => 20,
        Level::Debug => 10,
        Level::Trace => 0,
    }
}

fn is_enabled_for(logger: &PyAny, level: Level) -> PyResult<bool> {
    let level = map_level(level);
    logger.call_method1("isEnabledFor", (level,))?.is_true()
}

fn extract_max_level(py: Python<'_>, logger: &PyObject) -> PyResult<LevelFilter> {
    use Level::*;
    let logger = logger.as_ref(py);
    for l in &[Trace, Debug, Info, Warn, Error] {
        if is_enabled_for(logger, *l)? {
            return Ok(l.to_level_filter());
        }
    }

    Ok(LevelFilter::Off)
}

pub fn try_init() -> Result<ResetHandle, SetLoggerError> {
    Logger::default().install()
}

pub fn init() -> ResetHandle {
    try_init().unwrap()
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
