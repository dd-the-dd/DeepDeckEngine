use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    model::{PlayableCardInput, compile_playable_card},
};
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
    #[serde(default, deserialize_with = "deserialize_present_rules")]
    rules: Option<Vec<Value>>,
    #[serde(default)]
    input: Option<PlayableCardInput>,
    #[serde(default, rename = "expectedParserStatus")]
    expected_parser_status: Option<String>,
    #[serde(default, rename = "expectedEngineStatus")]
    expected_engine_status: Option<String>,
}

fn deserialize_present_rules<'de, D>(deserializer: D) -> Result<Option<Vec<Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Missing and explicitly empty differ for input/rules conflicts, while an
    // explicit null remains invalid as it was for the former Vec field.
    Vec::<Value>::deserialize(deserializer).map(Some)
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
        #[serde(rename = "actualParserStatus", skip_serializing_if = "Option::is_none")]
        actual_parser_status: Option<&'a str>,
        #[serde(rename = "actualEngineStatus", skip_serializing_if = "Option::is_none")]
        actual_engine_status: Option<&'a str>,
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
    let worker = std::thread::Builder::new()
        .name("card-contract-fixtures".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(run);
    match worker {
        Ok(worker) => worker.join().unwrap_or(ExitCode::FAILURE),
        Err(error) => {
            write_event(&OutputEvent::FixtureError {
                schema_version: RESULT_SCHEMA_VERSION,
                fixture_path: String::new(),
                error: format!("cannot start card contract worker: {error}"),
            });
            write_summary(&Summary {
                invalid_fixtures: 1,
                ..Summary::default()
            });
            ExitCode::FAILURE
        }
    }
}

fn run() -> ExitCode {
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

    let evaluation = evaluate_fixture(&fixture);
    let actual_executable = evaluation.executable;
    let failure = evaluation.failure(&fixture);
    let passed = failure.is_none();
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
        actual_parser_status: evaluation.parser_status.as_deref(),
        actual_engine_status: evaluation.engine_status.as_deref(),
        status: if passed {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        error: failure.as_deref(),
    });
}

#[derive(Debug)]
struct ContractEvaluation {
    executable: bool,
    parser_status: Option<String>,
    engine_status: Option<String>,
    error: Option<String>,
}

impl ContractEvaluation {
    fn failure(&self, fixture: &ContractFixture) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        if self.executable != fixture.expected_executable {
            return Some(format!(
                "executability mismatch: expected {}, actual {}",
                fixture.expected_executable, self.executable
            ));
        }
        for (name, expected, actual) in [
            (
                "parser",
                fixture.expected_parser_status.as_deref(),
                self.parser_status.as_deref(),
            ),
            (
                "engine",
                fixture.expected_engine_status.as_deref(),
                self.engine_status.as_deref(),
            ),
        ] {
            if let Some(expected) = expected {
                if actual != Some(expected) {
                    return Some(format!(
                        "{name} status mismatch: expected {expected:?}, actual {actual:?}"
                    ));
                }
            }
        }
        None
    }
}

fn evaluate_fixture(fixture: &ContractFixture) -> ContractEvaluation {
    if let Some(input) = &fixture.input {
        return match compile_playable_card(input.clone()) {
            Ok(compiled) => ContractEvaluation {
                // A card's compiler status includes unsupported source abilities;
                // an empty filtered rule list is never an executability proof.
                executable: compiled.engine_status == "executable",
                parser_status: Some(compiled.parser_status),
                engine_status: Some(compiled.engine_status),
                error: None,
            },
            Err(error) => ContractEvaluation {
                executable: false,
                parser_status: None,
                engine_status: None,
                error: Some(format!("card compilation failed: {error}")),
            },
        };
    }
    ContractEvaluation {
        executable: fixture
            .rule
            .iter()
            .chain(fixture.rules.as_deref().unwrap_or_default().iter())
            .all(rule_is_executable),
        parser_status: None,
        engine_status: None,
        error: None,
    }
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
    if fixture.input.is_some() {
        if fixture.rule.is_some() || fixture.rules.is_some() {
            return Err("fixture input conflicts with explicit rule/rules".to_string());
        }
        return Ok(());
    }
    if fixture.expected_parser_status.is_some() || fixture.expected_engine_status.is_some() {
        return Err("expectedParserStatus/expectedEngineStatus require input".to_string());
    }
    match (
        fixture.rule.is_some(),
        fixture.rules.as_ref().is_none_or(Vec::is_empty),
    ) {
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

#[cfg(test)]
#[path = "contract_input_tests.rs"]
mod tests;
