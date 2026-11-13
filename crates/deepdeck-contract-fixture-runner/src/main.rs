use deepdeck_engine_runtime::engine::rule_is_executable;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

const FIXTURE_SCHEMA_VERSION: &str = "deepdeck-contract-fixture/v1";
const RESULT_SCHEMA_VERSION: &str = "deepdeck-contract-fixture-result/v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractFixture {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    id: String,
    #[serde(default, rename = "cardName")]
    card_name: Option<String>,
    #[serde(default, rename = "oracleId")]
    oracle_id: Option<String>,
    #[serde(rename = "expectedExecutable")]
    expected_executable: bool,
    #[serde(default)]
    rule: Option<Value>,
    #[serde(default)]
    rules: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutputEvent<'a> {
    #[serde(rename = "fixtureResult")]
    FixtureResult {
        #[serde(rename = "schemaVersion")]
        schema_version: &'static str,
        #[serde(rename = "fixturePath")]
        fixture_path: String,
        #[serde(rename = "fixtureId")]
        fixture_id: &'a str,
        #[serde(rename = "cardName")]
        card_name: Option<&'a str>,
        #[serde(rename = "oracleId")]
        oracle_id: Option<&'a str>,
        #[serde(rename = "expectedExecutable")]
        expected_executable: bool,
        #[serde(rename = "actualExecutable")]
        actual_executable: bool,
        status: CaseStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    },
    #[serde(rename = "fixtureError")]
    FixtureError {
        #[serde(rename = "schemaVersion")]
        schema_version: &'static str,
        #[serde(rename = "fixturePath")]
        fixture_path: String,
        error: String,
    },
    #[serde(rename = "summary")]
    Summary {
        #[serde(rename = "schemaVersion")]
        schema_version: &'static str,
        #[serde(rename = "fixtureCount")]
        fixture_count: usize,
        passed: usize,
        failed: usize,
        #[serde(rename = "invalidFixtures")]
        invalid_fixtures: usize,
        status: SummaryStatus,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaseStatus {
    Passed,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum SummaryStatus {
    Passed,
    Failed,
}

#[derive(Default)]
struct Summary {
    fixture_count: usize,
    passed: usize,
    failed: usize,
    invalid_fixtures: usize,
}

impl Summary {
    fn succeeded(&self) -> bool {
        self.failed == 0 && self.invalid_fixtures == 0
    }
}

fn main() -> ExitCode {
    let inputs = env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        write_event(&OutputEvent::FixtureError {
            schema_version: RESULT_SCHEMA_VERSION,
            fixture_path: String::new(),
            error: "usage: contract_fixture_runner <fixture.json|fixture-directory> [...]"
                .to_string(),
        });
        write_summary(&Summary {
            invalid_fixtures: 1,
            ..Summary::default()
        });
        return ExitCode::FAILURE;
    }

    let (fixture_paths, discovery_errors) = discover_fixture_paths(&inputs);
    let mut summary = Summary::default();
    for (path, error) in discovery_errors {
        emit_fixture_error(&path, error, &mut summary);
    }

    if fixture_paths.is_empty() && summary.invalid_fixtures == 0 {
        emit_fixture_error(
            Path::new(""),
            "no JSON fixture was found in the supplied inputs".to_string(),
            &mut summary,
        );
    }

    let mut fixture_ids = HashSet::new();
    for path in fixture_paths {
        validate_fixture(&path, &mut summary, &mut fixture_ids);
    }

    write_summary(&summary);
    if summary.succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn discover_fixture_paths(inputs: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut fixtures = Vec::new();
    let mut errors = Vec::new();
    for input in inputs {
        collect_fixture_paths(input, &mut fixtures, &mut errors);
    }
    fixtures.sort();
    fixtures.dedup();
    (fixtures, errors)
}

fn collect_fixture_paths(
    path: &Path,
    fixtures: &mut Vec<PathBuf>,
    errors: &mut Vec<(PathBuf, String)>,
) {
    if path.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            fixtures.push(path.to_path_buf());
        } else {
            errors.push((
                path.to_path_buf(),
                "fixture file must use .json".to_string(),
            ));
        }
        return;
    }

    if !path.is_dir() {
        errors.push((
            path.to_path_buf(),
            "fixture path does not exist".to_string(),
        ));
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push((
                path.to_path_buf(),
                format!("cannot read directory: {error}"),
            ));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let entry_path = entry.path();
                if entry_path.is_dir()
                    || entry_path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                {
                    collect_fixture_paths(&entry_path, fixtures, errors);
                }
            }
            Err(error) => errors.push((
                path.to_path_buf(),
                format!("cannot read directory entry: {error}"),
            )),
        }
    }
}

fn validate_fixture(path: &Path, summary: &mut Summary, fixture_ids: &mut HashSet<String>) {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            emit_fixture_error(path, format!("cannot read fixture: {error}"), summary);
            return;
        }
    };
    let fixture: ContractFixture = match serde_json::from_str(&source) {
        Ok(fixture) => fixture,
        Err(error) => {
            emit_fixture_error(path, format!("invalid fixture JSON: {error}"), summary);
            return;
        }
    };
    if let Err(error) = validate_fixture_metadata(&fixture) {
        emit_fixture_error(path, error, summary);
        return;
    }
    if !fixture_ids.insert(fixture.id.clone()) {
        emit_fixture_error(
            path,
            format!("duplicate fixture id {:?}", fixture.id),
            summary,
        );
        return;
    }

    let actual_executable = fixture
        .rule
        .iter()
        .chain(fixture.rules.iter())
        .all(rule_is_executable);
    let passed = actual_executable == fixture.expected_executable;
    summary.fixture_count += 1;
    if passed {
        summary.passed += 1;
    } else {
        summary.failed += 1;
    }
    write_event(&OutputEvent::FixtureResult {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_path: path.display().to_string(),
        fixture_id: &fixture.id,
        card_name: fixture.card_name.as_deref(),
        oracle_id: fixture.oracle_id.as_deref(),
        expected_executable: fixture.expected_executable,
        actual_executable,
        status: if passed {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        error: None,
    });
}

fn validate_fixture_metadata(fixture: &ContractFixture) -> Result<(), String> {
    if fixture.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema_version {:?}; expected {FIXTURE_SCHEMA_VERSION:?}",
            fixture.schema_version
        ));
    }
    if fixture.id.trim().is_empty() {
        return Err("id must not be empty".to_string());
    }
    match (fixture.rule.is_some(), fixture.rules.is_empty()) {
        (true, true) | (false, false) => {}
        (true, false) => {
            return Err("fixture must provide either rule or rules, not both".to_string());
        }
        (false, true) => {
            return Err("fixture must provide a rule or a non-empty rules list".to_string());
        }
    }
    Ok(())
}

fn emit_fixture_error(path: &Path, error: String, summary: &mut Summary) {
    summary.invalid_fixtures += 1;
    write_event(&OutputEvent::FixtureError {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_path: path.display().to_string(),
        error,
    });
}

fn write_summary(summary: &Summary) {
    write_event(&OutputEvent::Summary {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_count: summary.fixture_count,
        passed: summary.passed,
        failed: summary.failed,
        invalid_fixtures: summary.invalid_fixtures,
        status: if summary.succeeded() {
            SummaryStatus::Passed
        } else {
            SummaryStatus::Failed
        },
    });
}

fn write_event(event: &OutputEvent<'_>) {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, event).expect("serializing an output event cannot fail");
    output
        .write_all(b"\n")
        .expect("writing a contract result to stdout cannot fail");
}
