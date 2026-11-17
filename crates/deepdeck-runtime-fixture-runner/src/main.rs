use deepdeck_runtime_fixture_runner::{FixtureStatus, RESULT_SCHEMA_VERSION, run_fixture_path};
use std::env;
use std::path::Path;

fn main() {
    let paths = env::args().skip(1).collect::<Vec<_>>();
    if paths.is_empty() {
        eprintln!("usage: deepdeck-runtime-fixture-runner <fixture.json> [fixture.json ...]");
        std::process::exit(2);
    }

    let worker = std::thread::Builder::new()
        .name("runtime-fixtures".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run(paths));
    let status = match worker {
        Ok(worker) => worker.join().unwrap_or_else(|_| {
            report_worker_failure("runtime fixture worker panicked");
            1
        }),
        Err(error) => {
            report_worker_failure(&format!("runtime fixture worker could not start: {error}"));
            1
        }
    };
    if status != 0 {
        std::process::exit(status);
    }
}

fn report_worker_failure(error: &str) {
    eprintln!(
        "{}",
        serde_json::json!({
            "schemaVersion": RESULT_SCHEMA_VERSION,
            "fixtureId": "runtime-fixture-runner",
            "status": "failed",
            "error": error,
        })
    );
}

fn run(paths: Vec<String>) -> i32 {
    let mut failed = false;
    for path in paths {
        let result = run_fixture_path(Path::new(&path));
        failed |= matches!(&result.status, FixtureStatus::Failed);
        println!(
            "{}",
            serde_json::to_string(&result).expect("fixture result serializes")
        );
    }
    if failed { 1 } else { 0 }
}
