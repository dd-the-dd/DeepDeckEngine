use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardFace, OracleCardParseRequest, OracleCardParseResult, parse_oracle_card},
};
use serde::Deserialize;
use serde_json::Value;
use std::{env, ffi::OsStr, fs, process, thread, time::Instant};

#[derive(Clone, Debug, Deserialize)]
struct ScryfallFace {
    #[serde(default)]
    oracle_id: Option<String>,
    name: String,
    type_line: String,
    #[serde(default)]
    mana_cost: Option<String>,
    #[serde(default)]
    oracle_text: Option<String>,
    #[serde(default)]
    power: Option<String>,
    #[serde(default)]
    toughness: Option<String>,
    #[serde(default)]
    loyalty: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ScryfallCard {
    id: String,
    name: String,
    collector_number: String,
    layout: String,
    #[serde(default)]
    type_line: String,
    #[serde(default)]
    mana_cost: Option<String>,
    #[serde(default)]
    oracle_text: Option<String>,
    #[serde(default)]
    card_faces: Vec<ScryfallFace>,
}

#[derive(Debug, Eq, PartialEq)]
enum ContractStatus {
    Executable,
    Unexecutable(usize),
    Unsupported,
}

fn parse_request(card: &ScryfallCard) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: card.name.clone(),
        type_line: card.type_line.clone(),
        mana_cost: card.mana_cost.clone(),
        oracle_text: card.oracle_text.clone(),
        layout: Some(card.layout.clone()),
        faces: card
            .card_faces
            .iter()
            .enumerate()
            .map(|(index, face)| OracleCardFace {
                id: face
                    .oracle_id
                    .clone()
                    .unwrap_or_else(|| format!("{}:face-{index}", card.id)),
                name: face.name.clone(),
                type_line: face.type_line.clone(),
                mana_cost: face.mana_cost.clone(),
                oracle_text: face.oracle_text.clone().unwrap_or_default(),
                power: face.power.clone(),
                toughness: face.toughness.clone(),
                loyalty: face.loyalty.clone(),
            })
            .collect(),
    }
}

fn load_cards(input: &OsStr) -> Vec<ScryfallCard> {
    let contents = fs::read(input).unwrap_or_else(|error| {
        eprintln!("could not read {}: {error}", input.to_string_lossy());
        process::exit(2);
    });
    let contents = contents
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(&contents);
    serde_json::from_slice(contents).unwrap_or_else(|error| {
        eprintln!("invalid Scryfall card array: {error}");
        process::exit(2);
    })
}

fn classify_rules<'a>(
    parser_status: &str,
    rules: impl IntoIterator<Item = Option<&'a Value>>,
) -> ContractStatus {
    if parser_status != "canonical" {
        return ContractStatus::Unsupported;
    }
    let rules = rules.into_iter().collect::<Vec<_>>();
    if rules.is_empty() {
        return ContractStatus::Unexecutable(1);
    }
    let unexecutable = rules
        .into_iter()
        .filter(|rule| rule.is_none_or(|rule| !rule_is_executable(rule)))
        .count();
    if unexecutable == 0 {
        ContractStatus::Executable
    } else {
        ContractStatus::Unexecutable(unexecutable)
    }
}

fn classify_parse(parsed: &OracleCardParseResult) -> ContractStatus {
    classify_rules(
        &parsed.status,
        parsed.abilities.iter().map(|ability| ability.rule.as_ref()),
    )
}

fn run() {
    let input = env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("usage: audit_contract_set <scryfall-card-array.json>");
        process::exit(2);
    });
    let cards = load_cards(&input);
    eprintln!("checking {} cards against runtime contracts", cards.len());
    let started = Instant::now();
    let mut executable = 0_usize;
    let mut unexecutable = 0_usize;
    let mut unsupported = 0_usize;

    for card in &cards {
        let parsed = parse_oracle_card(parse_request(card));
        match classify_parse(&parsed) {
            ContractStatus::Executable => {
                executable += 1;
                println!(
                    "CONTRACT\t{}\texecutable\t{}\t0",
                    card.collector_number, card.name
                );
            }
            ContractStatus::Unexecutable(count) => {
                unexecutable += 1;
                println!(
                    "CONTRACT\t{}\tunexecutable\t{}\t{count}",
                    card.collector_number, card.name
                );
            }
            ContractStatus::Unsupported => {
                unsupported += 1;
                println!(
                    "CONTRACT\t{}\tunsupported\t{}\t0",
                    card.collector_number, card.name
                );
            }
        }
    }

    println!(
        "CONTRACT_SUMMARY\texecutable={executable}\tunexecutable={unexecutable}\tunsupported={unsupported}\ttotal={}\telapsed={}us",
        cards.len(),
        started.elapsed().as_micros()
    );
    if executable != cards.len() {
        process::exit(1);
    }
}

fn main() {
    thread::Builder::new()
        .name("deepdeck-contract-audit".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("contract audit thread starts")
        .join()
        .expect("contract audit thread completes");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_executable_primitive_rule() {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: "Generic Life Spell".to_string(),
            type_line: "Instant".to_string(),
            mana_cost: Some("{W}".to_string()),
            oracle_text: Some("You gain 3 life.".to_string()),
            layout: None,
            faces: Vec::new(),
        });
        assert_eq!(classify_parse(&parsed), ContractStatus::Executable);
    }

    #[test]
    fn rejects_unknown_rule_kind() {
        let rule = json!({ "kind": "unknownTestKind" });
        assert_eq!(
            classify_rules("canonical", [Some(&rule)]),
            ContractStatus::Unexecutable(1)
        );
    }

    #[test]
    fn rejects_missing_rule_and_unsupported_parse() {
        assert_eq!(
            classify_rules("canonical", [None]),
            ContractStatus::Unexecutable(1)
        );
        assert_eq!(
            classify_rules("unsupported", std::iter::empty()),
            ContractStatus::Unsupported
        );
    }

    #[test]
    fn rejects_canonical_parse_without_rules() {
        assert_eq!(
            classify_rules("canonical", std::iter::empty()),
            ContractStatus::Unexecutable(1)
        );
    }
}
