//! In-process request dedupe: one computation per key, shared by every caller.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// Single-flight memo for the requests of one adapter run.
///
/// The first caller for a key runs the closure; concurrent callers for the same
/// key block until it finishes and every caller gets a clone of the one result.
/// Nothing is persisted: the memo lives as long as the value does.
#[derive(Debug)]
pub struct Flight<V> {
    cells: Mutex<HashMap<String, Arc<OnceLock<V>>>>,
}

impl<V> Default for Flight<V> {
    fn default() -> Self {
        Self {
            cells: Mutex::new(HashMap::new()),
        }
    }
}

impl<V: Clone> Flight<V> {
    /// An empty memo.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The result for `key`, computed by `compute` only if no caller has yet.
    pub fn get_or_init(&self, key: &str, compute: impl FnOnce() -> V) -> V {
        let cell = {
            let mut cells = self.cells.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(cells.entry(key.to_owned()).or_default())
        };
        cell.get_or_init(compute).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn concurrent_callers_share_one_computation() {
        let flight = Flight::<usize>::new();
        let runs = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let value = flight.get_or_init("k", || {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        runs.fetch_add(1, Ordering::SeqCst) + 7
                    });
                    assert_eq!(value, 7, "every caller sees the first result");
                });
            }
        });
        assert_eq!(runs.load(Ordering::SeqCst), 1, "computed once");
        assert_eq!(flight.get_or_init("other", || 1), 1, "keys are independent");
    }
}
