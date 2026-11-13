use super::*;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

#[test]
fn repeated_grammar_patterns_are_admitted_after_probation() {
    let cache = RegexCache::new(8, 8);
    let pattern = r"(?i)^target creature gets \+\d+/\+\d+$";

    cache.compile(pattern).expect("first compile succeeds");
    assert_eq!(
        cache.metrics(),
        RegexCacheMetrics {
            cached_patterns: 0,
            probation_patterns: 1,
            hits: 0,
            compilations: 1,
            admissions: 0,
            capacity_rejections: 0,
        }
    );

    cache.compile(pattern).expect("second compile succeeds");
    assert_eq!(cache.metrics().cached_patterns, 1);
    assert_eq!(cache.metrics().probation_patterns, 0);
    assert_eq!(cache.metrics().admissions, 1);

    cache.compile(pattern).expect("warm lookup succeeds");
    assert_eq!(cache.metrics().hits, 1);
    assert_eq!(cache.metrics().compilations, 2);
}

#[test]
fn unique_face_patterns_remain_bounded_and_are_not_cached() {
    let cache = RegexCache::new(4, 7);
    for index in 0..256 {
        let pattern = format!(r"(?i)^Card Name {index} deals \d+ damage$");
        cache.compile(&pattern).expect("dynamic regex compiles");
    }

    let metrics = cache.metrics();
    assert_eq!(metrics.cached_patterns, 0);
    assert_eq!(metrics.probation_patterns, 7);
    assert_eq!(metrics.compilations, 256);
}

#[test]
fn compiled_cache_is_bounded_and_retains_admitted_patterns() {
    let cache = RegexCache::new(3, 4);
    for pattern in ["^alpha$", "^beta$", "^gamma$"] {
        cache.compile(pattern).expect("probation compile succeeds");
        cache.compile(pattern).expect("admission compile succeeds");
    }
    for index in 0..100 {
        let pattern = format!("^dynamic-{index}$");
        cache.compile(&pattern).expect("dynamic regex compiles");
        cache.compile(&pattern).expect("repeat compiles");
    }

    let before = cache.metrics();
    assert_eq!(before.cached_patterns, 3);
    assert!(before.probation_patterns <= 4);
    assert_eq!(before.capacity_rejections, 100);

    for pattern in ["^alpha$", "^beta$", "^gamma$"] {
        cache.compile(pattern).expect("admitted regex remains warm");
    }
    let after = cache.metrics();
    assert_eq!(after.cached_patterns, 3);
    assert_eq!(after.hits, before.hits + 3);
    assert_eq!(after.compilations, before.compilations);
}

#[test]
fn concurrent_admission_keeps_both_structures_bounded() {
    const WORKERS: usize = 8;
    let cache = Arc::new(RegexCache::new(2, 5));
    let barrier = Arc::new(Barrier::new(WORKERS));
    let workers = (0..WORKERS)
        .map(|worker| {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..4 {
                    cache
                        .compile(r"(?i)^stable grammar pattern$")
                        .expect("shared grammar regex compiles");
                }
                cache
                    .compile(&format!(r"(?i)^Unique Face {worker}$"))
                    .expect("unique face regex compiles");
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().expect("cache worker completes");
    }

    let metrics = cache.metrics();
    assert_eq!(metrics.cached_patterns, 1);
    assert!(metrics.probation_patterns <= 5);
    assert!(metrics.hits > 0);
}

#[test]
#[ignore = "diagnostic benchmark"]
fn reports_cold_compile_and_warm_lookup_timings() {
    const ITERATIONS: usize = 2_000;
    let pattern = r"(?i)^(?:target|each) [A-Za-z ]+ gets [+-]?\d+/[+-]?\d+$";

    let cold_started = Instant::now();
    for _ in 0..ITERATIONS {
        regex::Regex::new(pattern).expect("cold regex compiles");
    }
    let cold = cold_started.elapsed();

    let cache = RegexCache::new(8, 8);
    cache.compile(pattern).expect("probation compile succeeds");
    cache.compile(pattern).expect("admission compile succeeds");
    let warm_started = Instant::now();
    for _ in 0..ITERATIONS {
        cache.compile(pattern).expect("warm regex lookup succeeds");
    }
    let warm = warm_started.elapsed();

    eprintln!(
        "regex-cache microbenchmark: iterations={ITERATIONS} cold_compile={cold:?} warm_lookup={warm:?}"
    );
    assert_eq!(cache.metrics().hits, ITERATIONS);
    assert_eq!(cache.metrics().cached_patterns, 1);
}
