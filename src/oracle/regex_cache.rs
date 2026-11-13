use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, RwLock};

const MAX_CACHED_PATTERNS: usize = 2_048;
const MAX_PROBATION_PATTERNS: usize = 4_096;

/// Drop-in constructor for the regex crate that shares frequently reused
/// compiled programs across Oracle parses.
///
/// Grammar patterns are admitted after their second observation. One-off
/// patterns, including patterns parameterized by a card face name, remain in a
/// bounded probation queue and therefore cannot grow the compiled cache with
/// the size of the card corpus.
pub(crate) struct Regex;

impl Regex {
    pub(crate) fn new(pattern: &str) -> Result<regex::Regex, regex::Error> {
        global_cache().compile(pattern)
    }
}

struct RegexCache {
    state: RwLock<CacheState>,
    hits: AtomicUsize,
    compilations: AtomicUsize,
    admissions: AtomicUsize,
    capacity_rejections: AtomicUsize,
}

impl RegexCache {
    fn new(cache_capacity: usize, probation_capacity: usize) -> Self {
        Self {
            state: RwLock::new(CacheState::new(cache_capacity, probation_capacity)),
            hits: AtomicUsize::new(0),
            compilations: AtomicUsize::new(0),
            admissions: AtomicUsize::new(0),
            capacity_rejections: AtomicUsize::new(0),
        }
    }

    fn compile(&self, pattern: &str) -> Result<regex::Regex, regex::Error> {
        if let Some(compiled) = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .compiled
            .get(pattern)
            .cloned()
        {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(compiled);
        }

        let compiled = regex::Regex::new(pattern)?;
        self.compilations.fetch_add(1, Ordering::Relaxed);

        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = state.compiled.get(pattern).cloned() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(existing);
        }

        match state.observe(pattern, &compiled) {
            Admission::Admitted => {
                self.admissions.fetch_add(1, Ordering::Relaxed);
                Ok(state
                    .compiled
                    .get(pattern)
                    .expect("admitted regex is present")
                    .clone())
            }
            Admission::RejectedAtCapacity => {
                self.capacity_rejections.fetch_add(1, Ordering::Relaxed);
                Ok(compiled)
            }
            Admission::Probation => Ok(compiled),
        }
    }

    #[cfg(test)]
    fn metrics(&self) -> RegexCacheMetrics {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        RegexCacheMetrics {
            cached_patterns: state.compiled.len(),
            probation_patterns: state.probation.len(),
            hits: self.hits.load(Ordering::Relaxed),
            compilations: self.compilations.load(Ordering::Relaxed),
            admissions: self.admissions.load(Ordering::Relaxed),
            capacity_rejections: self.capacity_rejections.load(Ordering::Relaxed),
        }
    }
}

struct CacheState {
    compiled: HashMap<String, regex::Regex>,
    probation: HashSet<String>,
    probation_order: VecDeque<String>,
    cache_capacity: usize,
    probation_capacity: usize,
}

impl CacheState {
    fn new(cache_capacity: usize, probation_capacity: usize) -> Self {
        Self {
            compiled: HashMap::new(),
            probation: HashSet::new(),
            probation_order: VecDeque::new(),
            cache_capacity,
            probation_capacity,
        }
    }

    fn observe(&mut self, pattern: &str, compiled: &regex::Regex) -> Admission {
        if self.probation.remove(pattern) {
            if let Some(index) = self
                .probation_order
                .iter()
                .position(|candidate| candidate == pattern)
            {
                self.probation_order.remove(index);
            }
            if self.compiled.len() >= self.cache_capacity {
                return Admission::RejectedAtCapacity;
            }
            self.compiled.insert(pattern.to_string(), compiled.clone());
            return Admission::Admitted;
        }

        if self.probation_capacity == 0 {
            return Admission::Probation;
        }
        while self.probation.len() >= self.probation_capacity {
            let Some(oldest) = self.probation_order.pop_front() else {
                break;
            };
            self.probation.remove(&oldest);
        }
        let owned = pattern.to_string();
        self.probation.insert(owned.clone());
        self.probation_order.push_back(owned);
        Admission::Probation
    }
}

enum Admission {
    Admitted,
    RejectedAtCapacity,
    Probation,
}

fn global_cache() -> &'static RegexCache {
    static CACHE: OnceLock<RegexCache> = OnceLock::new();
    CACHE.get_or_init(|| RegexCache::new(MAX_CACHED_PATTERNS, MAX_PROBATION_PATTERNS))
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RegexCacheMetrics {
    cached_patterns: usize,
    probation_patterns: usize,
    hits: usize,
    compilations: usize,
    admissions: usize,
    capacity_rejections: usize,
}

#[cfg(test)]
#[path = "regex_cache_tests.rs"]
mod tests;
