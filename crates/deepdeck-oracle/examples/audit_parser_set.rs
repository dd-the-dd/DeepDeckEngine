use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance,
    parse_oracle_card, parse_oracle_card_native,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    env,
    ffi::OsStr,
    fs,
    io::{self, Write},
    process, thread,
    time::Instant,
};

fn exit_with_flushed_stdout(code: i32) -> ! {
    let _ = io::stdout().flush();
    process::exit(code);
}

const DEFAULT_FORBIDDEN_IR_KINDS: &[&str] = &[
    "resolveTriggeredInstruction",
    "resolveSpellInstruction",
    "discardThenDraw",
    "drawThenDiscard",
    "exileThenReturnTransformed",
    "sacrificeThenSearchLibrary",
    "tapOrUntapPermanent",
];

#[derive(Debug, Eq, PartialEq)]
enum IrArchitecture {
    Primitive,
    Composite(Vec<String>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CardRecognizerStatus {
    Native,
    Mixed,
    Legacy,
    Unsupported,
}

impl CardRecognizerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Mixed => "mixed",
            Self::Legacy => "legacy",
            Self::Unsupported => "unsupported",
        }
    }

    fn is_native(self) -> bool {
        self == Self::Native
    }
}

fn should_stop_at_first_non_native(
    strict_prefix: bool,
    recognizer_status: CardRecognizerStatus,
) -> bool {
    strict_prefix && !recognizer_status.is_native()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AbilityRecognizerCounts {
    native: usize,
    legacy: usize,
    unclaimed: usize,
}

fn classify_recognizers(
    parsed: &OracleCardParseResult,
) -> (CardRecognizerStatus, AbilityRecognizerCounts) {
    let mut counts = AbilityRecognizerCounts::default();
    for ability in &parsed.abilities {
        match ability.recognizer {
            RecognizerProvenance::Native => counts.native += 1,
            RecognizerProvenance::Legacy => counts.legacy += 1,
            RecognizerProvenance::Unclaimed => counts.unclaimed += 1,
        }
    }

    let status = if parsed.status != "canonical" {
        CardRecognizerStatus::Unsupported
    } else if !parsed.abilities.is_empty() && counts.native == parsed.abilities.len() {
        CardRecognizerStatus::Native
    } else if counts.native > 0 {
        // Any legacy ability keeps the entire card out of the full-native
        // counter, even though one or more abilities use promoted automata.
        CardRecognizerStatus::Mixed
    } else {
        // Empty-rules cards are deliberately not certified as native.
        CardRecognizerStatus::Legacy
    };
    (status, counts)
}

fn forbidden_ir_kinds() -> HashSet<String> {
    let configured = env::var("AUDIT_FORBIDDEN_KINDS").ok();
    let kinds = configured
        .as_deref()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .map(str::to_string)
                .collect::<HashSet<_>>()
        })
        .filter(|kinds| !kinds.is_empty());
    kinds.unwrap_or_else(|| {
        DEFAULT_FORBIDDEN_IR_KINDS
            .iter()
            .map(|kind| (*kind).to_string())
            .collect()
    })
}

fn collect_forbidden_kinds(
    value: &Value,
    forbidden: &HashSet<String>,
    found: &mut BTreeSet<String>,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_forbidden_kinds(value, forbidden, found);
            }
        }
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str)
                && forbidden.contains(kind)
            {
                found.insert(kind.to_string());
            }
            for value in object.values() {
                collect_forbidden_kinds(value, forbidden, found);
            }
        }
        _ => {}
    }
}

fn classify_rule_values<'a>(
    rules: impl IntoIterator<Item = Option<&'a Value>>,
    forbidden: &HashSet<String>,
) -> IrArchitecture {
    let mut found = BTreeSet::new();
    for rule in rules {
        if let Some(rule) = rule {
            collect_forbidden_kinds(rule, forbidden, &mut found);
        } else {
            found.insert("missingCanonicalRule".to_string());
        }
    }
    if found.is_empty() {
        IrArchitecture::Primitive
    } else {
        IrArchitecture::Composite(found.into_iter().collect())
    }
}

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
    #[serde(default)]
    audit_key: Option<String>,
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

impl ScryfallCard {
    fn audit_identity(&self) -> &str {
        self.audit_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.collector_number)
    }
}

#[derive(Debug, Deserialize)]
struct ScryfallList {
    data: Vec<ScryfallCard>,
    has_more: bool,
    #[serde(default)]
    next_page: Option<String>,
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
    let input = input.to_string_lossy();
    if let Some(set_code) = input.strip_prefix("set:") {
        let mut url = format!(
            "https://api.scryfall.com/cards/search?q=set%3A{}&order=set&unique=cards",
            set_code.trim().to_ascii_lowercase()
        );
        let mut cards = Vec::new();
        loop {
            let page: ScryfallList = ureq::get(&url)
                .set("User-Agent", "DeepDeck parser audit/0.1")
                .set("Accept", "application/json")
                .call()
                .unwrap_or_else(|error| {
                    eprintln!("could not fetch Scryfall set {set_code}: {error}");
                    process::exit(2);
                })
                .into_json()
                .unwrap_or_else(|error| {
                    eprintln!("invalid Scryfall response for set {set_code}: {error}");
                    process::exit(2);
                });
            cards.extend(page.data);
            if !page.has_more {
                break;
            }
            url = page.next_page.unwrap_or_else(|| {
                eprintln!("Scryfall response for set {set_code} omitted next_page");
                process::exit(2);
            });
        }
        return cards;
    }

    let contents = fs::read(input.as_ref()).unwrap_or_else(|error| {
        eprintln!("could not read {input}: {error}");
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

fn print_rules(card: &ScryfallCard, parsed: &OracleCardParseResult) {
    if env::var_os("AUDIT_RULES").is_none() {
        return;
    }
    for ability in &parsed.abilities {
        eprintln!(
            "{} {} | {} | {} | {}",
            card.audit_identity(),
            card.name,
            ability.status,
            ability.source.text.replace('\n', " "),
            ability
                .rule
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "<no rule>".to_string())
        );
    }
}

fn serialized_semantic_rules(parsed: &OracleCardParseResult) -> String {
    serde_json::to_string(
        &parsed
            .abilities
            .iter()
            .map(|ability| ability.rule.as_ref())
            .collect::<Vec<_>>(),
    )
    .expect("canonical rules serialize for proof fingerprinting")
}

fn run() {
    let input = env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("usage: audit_parser_set <set:code|scryfall-card-array.json>");
        process::exit(2);
    });
    let collector = env::var("AUDIT_COLLECTOR").ok();
    let native_only = env::var_os("AUDIT_NATIVE_ONLY").is_some();
    let stop_at_first_non_native = env::var_os("AUDIT_STOP_AT_FIRST_NON_NATIVE").is_some();
    let mut cards = load_cards(&input);
    if let Some(collector) = collector.as_deref() {
        cards.retain(|card| card.collector_number == collector);
    }
    eprintln!(
        "checking {} cards sequentially in Scryfall set order (parser mode={})",
        cards.len(),
        if native_only { "nativeOnly" } else { "full" }
    );

    let audit_started = Instant::now();
    let mut canonical = 0_usize;
    let mut primitive = 0_usize;
    let mut composite = 0_usize;
    let mut native_cards = 0_usize;
    let mut mixed_cards = 0_usize;
    let mut legacy_cards = 0_usize;
    let mut unsupported_recognizer_cards = 0_usize;
    let mut recognizer_abilities = AbilityRecognizerCounts::default();
    let forbidden = forbidden_ir_kinds();
    for card in &cards {
        let started = Instant::now();
        let request = parse_request(card);
        let parsed = if native_only {
            parse_oracle_card_native(request)
        } else {
            parse_oracle_card(request)
        };
        let (recognizer_status, ability_counts) = classify_recognizers(&parsed);
        recognizer_abilities.native += ability_counts.native;
        recognizer_abilities.legacy += ability_counts.legacy;
        recognizer_abilities.unclaimed += ability_counts.unclaimed;
        match recognizer_status {
            CardRecognizerStatus::Native => native_cards += 1,
            CardRecognizerStatus::Mixed => mixed_cards += 1,
            CardRecognizerStatus::Legacy => legacy_cards += 1,
            CardRecognizerStatus::Unsupported => unsupported_recognizer_cards += 1,
        }
        print_rules(card, &parsed);
        if parsed.status == "canonical" {
            canonical += 1;
            println!(
                "CARD\t{}\tcanonical\t{}\t{}us",
                card.audit_identity(),
                card.name,
                started.elapsed().as_micros()
            );
            println!(
                "RECOGNIZER\t{}\t{}\t{}\t{}\t{}\t{}",
                card.audit_identity(),
                recognizer_status.as_str(),
                ability_counts.native,
                ability_counts.legacy,
                ability_counts.unclaimed,
                card.name
            );
            match classify_rule_values(
                parsed.abilities.iter().map(|ability| ability.rule.as_ref()),
                &forbidden,
            ) {
                IrArchitecture::Primitive => {
                    primitive += 1;
                    println!("IR\t{}\tprimitive\t{}\t-", card.audit_identity(), card.name);
                }
                IrArchitecture::Composite(kinds) => {
                    composite += 1;
                    println!(
                        "IR\t{}\tcomposite\t{}\t{}",
                        card.audit_identity(),
                        card.name,
                        kinds.join(",")
                    );
                }
            }
            println!(
                "RULES\t{}\t{}",
                card.audit_identity(),
                serialized_semantic_rules(&parsed)
            );
            if should_stop_at_first_non_native(stop_at_first_non_native, recognizer_status) {
                println!(
                    "FIRST_NON_NATIVE\t{}\t{}\t{}",
                    card.audit_identity(),
                    recognizer_status.as_str(),
                    card.name
                );
                exit_with_flushed_stdout(1);
            }
            continue;
        }

        println!(
            "CARD\t{}\tunsupported\t{}\t{}us",
            card.audit_identity(),
            card.name,
            started.elapsed().as_micros()
        );
        println!(
            "RECOGNIZER\t{}\t{}\t{}\t{}\t{}\t{}",
            card.audit_identity(),
            recognizer_status.as_str(),
            ability_counts.native,
            ability_counts.legacy,
            ability_counts.unclaimed,
            card.name
        );
        println!(
            "IR\t{}\tunsupported\t{}\t-",
            card.audit_identity(),
            card.name
        );
        println!(
            "RULES\t{}\t{}",
            card.audit_identity(),
            serialized_semantic_rules(&parsed)
        );
        for ability in parsed
            .abilities
            .iter()
            .filter(|ability| ability.status != "canonical")
        {
            println!(
                "ABILITY\t{} {}\t{}",
                card.audit_identity(),
                card.name,
                ability.source.text.replace('\n', " ")
            );
        }
        if should_stop_at_first_non_native(stop_at_first_non_native, recognizer_status) {
            println!(
                "FIRST_NON_NATIVE\t{}\tparser-unsupported\t{}",
                card.audit_identity(),
                card.name
            );
            exit_with_flushed_stdout(1);
        }
        if env::var_os("AUDIT_CARD_BY_CARD").is_some() {
            println!(
                "FIRST_UNCOVERED\t{}\tparser-unsupported\t{}",
                card.audit_identity(),
                card.name
            );
            exit_with_flushed_stdout(1);
        }
    }

    println!(
        "SUMMARY\tmode={}\tcanonical={canonical}\tprimitive={primitive}\tcomposite={composite}\tnative={native_cards}\tmixed={mixed_cards}\tlegacy={legacy_cards}\tunsupportedRecognizer={unsupported_recognizer_cards}\tnativeAbilities={}\tlegacyAbilities={}\tunclaimedAbilities={}\ttotal={}\telapsed={}us",
        if native_only { "nativeOnly" } else { "full" },
        recognizer_abilities.native,
        recognizer_abilities.legacy,
        recognizer_abilities.unclaimed,
        cards.len(),
        audit_started.elapsed().as_micros()
    );
    if canonical != cards.len() {
        exit_with_flushed_stdout(1);
    }
}

#[cfg(test)]
#[path = "audit_parser_set_tests.rs"]
mod tests;

fn main() {
    thread::Builder::new()
        .name("deepdeck-parser-audit".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("parser audit thread starts")
        .join()
        .expect("parser audit thread completes");
}
