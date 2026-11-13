use deepdeck_parser_fixture_runner::{
    FixtureStatus, ParserMode, RESULT_SCHEMA_VERSION, run_fixture_paths_with_mode,
};
use std::{
    env,
    ffi::OsStr,
    path::PathBuf,
    process::ExitCode,
    thread::{self, JoinHandle},
};

const DEFAULT_WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
const WORKER_STACK_ENV: &str = "DEEPDECK_PARSER_FIXTURE_STACK_BYTES";
const RUST_STACK_ENV: &str = "RUST_MIN_STACK";

fn main() -> ExitCode {
    let stack_bytes = match configured_worker_stack_bytes() {
        Ok(stack_bytes) => stack_bytes,
        Err(error) => {
            report_runner_error(&error);
            return ExitCode::FAILURE;
        }
    };

    let worker = thread::Builder::new()
        .name("parser-fixtures".to_string())
        .stack_size(stack_bytes)
        .spawn(run);
    finish_worker(worker)
}

fn run() -> ExitCode {
    let mut parser_mode = ParserMode::Full;
    let mut inputs = Vec::new();
    for argument in env::args_os().skip(1) {
        if argument.as_os_str() == OsStr::new("--native-only") {
            parser_mode = ParserMode::NativeOnly;
        } else {
            inputs.push(PathBuf::from(argument));
        }
    }
    if inputs.is_empty() {
        eprintln!(
            "{}",
            serde_json::json!({
                "schemaVersion": RESULT_SCHEMA_VERSION,
                "type": "fixtureError",
                "fixturePath": "",
                "error": "usage: deepdeck-parser-fixture-runner [--native-only] <fixture.json|fixture-directory> [...]",
            })
        );
        return ExitCode::FAILURE;
    }

    let report = run_fixture_paths_with_mode(&inputs, parser_mode);
    for event in &report.events {
        println!(
            "{}",
            serde_json::to_string(event).expect("parser fixture event serializes")
        );
    }
    println!(
        "{}",
        serde_json::to_string(&report.summary).expect("parser fixture summary serializes")
    );

    if report.summary.status == FixtureStatus::Passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn configured_worker_stack_bytes() -> Result<usize, String> {
    resolve_worker_stack_bytes(
        env::var_os(WORKER_STACK_ENV).as_deref(),
        env::var_os(RUST_STACK_ENV).as_deref(),
    )
}

fn resolve_worker_stack_bytes(
    runner_stack: Option<&OsStr>,
    rust_stack: Option<&OsStr>,
) -> Result<usize, String> {
    if let Some(value) = runner_stack {
        return parse_stack_bytes(WORKER_STACK_ENV, value);
    }
    if let Some(value) = rust_stack {
        return parse_stack_bytes(RUST_STACK_ENV, value);
    }
    Ok(DEFAULT_WORKER_STACK_BYTES)
}

fn parse_stack_bytes(variable: &str, value: &OsStr) -> Result<usize, String> {
    let value = value
        .to_str()
        .ok_or_else(|| format!("{variable} must contain UTF-8 decimal bytes"))?;
    let bytes = value
        .parse::<usize>()
        .map_err(|_| format!("{variable} must be a positive decimal byte count, got {value:?}"))?;
    if bytes == 0 {
        return Err(format!("{variable} must be greater than zero"));
    }
    Ok(bytes)
}

fn finish_worker(worker: std::io::Result<JoinHandle<ExitCode>>) -> ExitCode {
    match worker {
        Ok(worker) => worker.join().unwrap_or_else(|_| {
            report_runner_error("parser fixture worker panicked");
            ExitCode::FAILURE
        }),
        Err(error) => {
            report_runner_error(&format!("parser fixture worker could not start: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn report_runner_error(error: &str) {
    eprintln!(
        "{}",
        serde_json::json!({
            "schemaVersion": RESULT_SCHEMA_VERSION,
            "type": "fixtureError",
            "fixturePath": "",
            "error": error,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_size_defaults_to_thirty_two_mibibytes() {
        assert_eq!(
            resolve_worker_stack_bytes(None, None).expect("default stack size"),
            32 * 1024 * 1024
        );
    }

    #[test]
    fn runner_specific_stack_size_overrides_rust_min_stack() {
        assert_eq!(
            resolve_worker_stack_bytes(Some(OsStr::new("67108864")), Some(OsStr::new("16777216")),)
                .expect("configured stack size"),
            64 * 1024 * 1024
        );
    }

    #[test]
    fn rust_min_stack_is_used_when_runner_override_is_absent() {
        assert_eq!(
            resolve_worker_stack_bytes(None, Some(OsStr::new("33554432")))
                .expect("RUST_MIN_STACK fallback"),
            32 * 1024 * 1024
        );
    }

    #[test]
    fn invalid_stack_size_is_reported_before_worker_start() {
        let error = resolve_worker_stack_bytes(Some(OsStr::new("0")), None)
            .expect_err("zero stack size must fail");
        assert!(error.contains("greater than zero"));
    }
}
