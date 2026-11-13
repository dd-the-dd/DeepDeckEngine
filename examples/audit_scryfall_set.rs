use mtg_engine::engine::rule_is_executable;
use mtg_engine::model::{PlayableCardInput, compile_playable_card};
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest};
use serde::Deserialize;
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
    power: Option<String>,
    #[serde(default)]
    toughness: Option<String>,
    #[serde(default)]
    card_faces: Vec<ScryfallFace>,
}

#[derive(Debug, Deserialize)]
struct ScryfallList {
    data: Vec<ScryfallCard>,
    has_more: bool,
    #[serde(default)]
    next_page: Option<String>,
}

#[derive(Debug)]
struct AuditOutcome {
    index: usize,
    pipeline_micros: u128,
    parser_canonical: bool,
    engine_executable: bool,
    parser_failures: Vec<String>,
    engine_failures: Vec<String>,
}

fn request(card: &ScryfallCard) -> OracleCardParseRequest {
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

fn audit_card(index: usize, card: &ScryfallCard) -> AuditOutcome {
    let pipeline_started = Instant::now();
    let oracle = request(card);
    let face_id = oracle.faces.first().map(|face| face.id.clone());
    let power = oracle
        .faces
        .first()
        .and_then(|face| face.power.clone())
        .or_else(|| card.power.clone());
    let toughness = oracle
        .faces
        .first()
        .and_then(|face| face.toughness.clone())
        .or_else(|| card.toughness.clone());
    let compilation = compile_playable_card(PlayableCardInput {
        id: card.id.clone(),
        face_id,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power,
        toughness,
        oracle,
    })
    .unwrap_or_else(|error| {
        panic!(
            "{} {} | compile error | {error}",
            card.collector_number, card.name
        )
    });
    let parsed = &compilation.parser_result;
    if env::var_os("AUDIT_VERBOSE").is_some() {
        eprintln!(
            "{} {}\n{}",
            card.collector_number,
            card.name,
            serde_json::to_string_pretty(parsed).expect("parser result serializes")
        );
    }
    if env::var_os("AUDIT_RULES").is_some() {
        for ability in &parsed.abilities {
            eprintln!(
                "{} {} | {} | engine={} | {} | {}",
                card.collector_number,
                card.name,
                ability.status,
                ability.rule.as_ref().is_some_and(rule_is_executable),
                ability.source.text.replace('\n', " "),
                ability
                    .rule
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<no rule>".to_string())
            );
        }
    }
    let parser_canonical = parsed.status == "canonical";
    let engine_executable = parser_canonical
        && parsed
            .abilities
            .iter()
            .all(|ability| ability.rule.as_ref().is_some_and(rule_is_executable));
    let parser_failures = if parser_canonical {
        Vec::new()
    } else {
        parsed
            .abilities
            .iter()
            .filter(|ability| ability.status != "canonical")
            .map(|ability| {
                format!(
                    "{} {} | {}",
                    card.collector_number,
                    card.name,
                    ability.source.text.replace('\n', " ")
                )
            })
            .collect()
    };
    let engine_failures = if parser_canonical && !engine_executable {
        parsed
            .abilities
            .iter()
            .filter(|ability| !ability.rule.as_ref().is_some_and(rule_is_executable))
            .map(|ability| {
                format!(
                    "{} {} | {}",
                    card.collector_number,
                    card.name,
                    ability.source.text.replace('\n', " ")
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    AuditOutcome {
        index,
        pipeline_micros: pipeline_started.elapsed().as_micros(),
        parser_canonical,
        engine_executable,
        parser_failures,
        engine_failures,
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
                .set("User-Agent", "DeepDeckEngine set audit/0.1")
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

fn audit_card_by_card(cards: &[ScryfallCard]) {
    eprintln!(
        "checking {} cards sequentially in Scryfall set order",
        cards.len()
    );
    for (index, card) in cards.iter().enumerate() {
        let outcome = audit_card(index, card);
        if outcome.engine_executable {
            println!(
                "CARD\t{}\tcovered\t{}\t{}us",
                card.collector_number, card.name, outcome.pipeline_micros
            );
            continue;
        }

        let status = if outcome.parser_canonical {
            "engine-unsupported"
        } else {
            "parser-unsupported"
        };
        println!(
            "FIRST_UNCOVERED\t{}\t{}\t{}\t{}us",
            card.collector_number, status, card.name, outcome.pipeline_micros
        );
        for failure in outcome
            .parser_failures
            .iter()
            .chain(outcome.engine_failures.iter())
        {
            println!("ABILITY\t{failure}");
        }
        return;
    }
    println!("SET_COMPLETE\t{}", cards.len());
}

fn audit() {
    let Some(input) = env::args_os().nth(1) else {
        eprintln!(
            "usage: cargo run --example audit_scryfall_set -- <scryfall-cards.json|set:code>"
        );
        process::exit(2);
    };
    let mut cards = load_cards(&input);
    if let Ok(collector_filter) = env::var("AUDIT_COLLECTOR") {
        let collectors = collector_filter
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        cards.retain(|card| collectors.contains(card.collector_number.as_str()));
    }
    if env::var_os("AUDIT_CARD_BY_CARD").is_some() {
        audit_card_by_card(&cards);
        return;
    }

    let worker_count = env::var("AUDIT_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map_or(1, usize::from)
                .min(4)
        })
        .min(cards.len().max(1));
    eprintln!("auditing {} cards with {worker_count} workers", cards.len());
    let audit_started = Instant::now();
    let mut outcomes = thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..worker_count {
            let cards = &cards;
            handles.push(
                thread::Builder::new()
                    .name(format!("set-audit-{worker}"))
                    .stack_size(64 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        cards
                            .iter()
                            .enumerate()
                            .skip(worker)
                            .step_by(worker_count)
                            .map(|(index, card)| audit_card(index, card))
                            .collect::<Vec<_>>()
                    })
                    .expect("set audit worker starts"),
            );
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("set audit worker completes"))
            .collect::<Vec<_>>()
    });
    let audit_micros = audit_started.elapsed().as_micros();
    outcomes.sort_by_key(|outcome| outcome.index);

    let mut pipeline_latencies = outcomes
        .iter()
        .map(|outcome| outcome.pipeline_micros)
        .collect::<Vec<_>>();
    pipeline_latencies.sort_unstable();
    let percentile = |percent: usize| -> u128 {
        if pipeline_latencies.is_empty() {
            return 0;
        }
        let numerator = percent * (pipeline_latencies.len() - 1);
        pipeline_latencies[(numerator + 99) / 100]
    };
    let maximum_pipeline_micros = pipeline_latencies.last().copied().unwrap_or_default();
    let cards_per_second = if audit_micros == 0 {
        0.0
    } else {
        cards.len() as f64 * 1_000_000.0 / audit_micros as f64
    };

    let parser_canonical = outcomes
        .iter()
        .filter(|outcome| outcome.parser_canonical)
        .count();
    let engine_executable = outcomes
        .iter()
        .filter(|outcome| outcome.engine_executable)
        .count();
    let parser_failures = outcomes
        .iter_mut()
        .flat_map(|outcome| std::mem::take(&mut outcome.parser_failures))
        .collect::<Vec<_>>();
    let engine_failures = outcomes
        .iter_mut()
        .flat_map(|outcome| std::mem::take(&mut outcome.engine_failures))
        .collect::<Vec<_>>();

    let mut report = format!(
        "total={}\nparser_canonical={parser_canonical}\nengine_executable={engine_executable}\nparser_failure_cards={}\nengine_failure_cards={}\naudit_elapsed_ms={:.3}\ncards_per_second={cards_per_second:.3}\npipeline_latency_us_p50={}\npipeline_latency_us_p95={}\npipeline_latency_us_p99={}\npipeline_latency_us_max={}\n\nPARSER FAILURES\n",
        cards.len(),
        cards.len() - parser_canonical,
        cards.len() - engine_executable,
        audit_micros as f64 / 1_000.0,
        percentile(50),
        percentile(95),
        percentile(99),
        maximum_pipeline_micros,
    );
    for failure in &parser_failures {
        report.push_str(failure);
        report.push('\n');
    }
    report.push_str("\nENGINE-ONLY FAILURES\n");
    for failure in &engine_failures {
        report.push_str(failure);
        report.push('\n');
    }
    if let Some(path) = env::var_os("AUDIT_REPORT") {
        fs::write(&path, &report).unwrap_or_else(|error| {
            eprintln!(
                "could not write audit report {}: {error}",
                path.to_string_lossy()
            );
            process::exit(2);
        });
        print!(
            "total={}\nparser_canonical={parser_canonical}\nengine_executable={engine_executable}\nparser_failure_cards={}\nengine_failure_cards={}\naudit_elapsed_ms={:.3}\ncards_per_second={cards_per_second:.3}\npipeline_latency_us_p50={}\npipeline_latency_us_p95={}\npipeline_latency_us_p99={}\npipeline_latency_us_max={}\nreport={}\n",
            cards.len(),
            cards.len() - parser_canonical,
            cards.len() - engine_executable,
            audit_micros as f64 / 1_000.0,
            percentile(50),
            percentile(95),
            percentile(99),
            maximum_pipeline_micros,
            path.to_string_lossy(),
        );
    } else {
        print!("{report}");
    }
}

fn main() {
    std::thread::Builder::new()
        .name("set-audit".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(audit)
        .expect("set audit thread starts")
        .join()
        .expect("set audit thread completes");
}
