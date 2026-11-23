//! Runtime smoke harness: does every supported card's executable rule do
//! what its IR promises when it is played?
//!
//! For each card the support probe marks supported, the harness compiles the
//! card, places it in a synthetic two-player game (creatures, an artifact, an
//! enchantment and a land on each side, cards in hands and graveyards), and
//! for each spell, activated or triggered rule takes the first legal
//! declaration (first targets, synthetic event bindings for a trigger),
//! resolves the rule with scripted decisions (first option, accept the
//! optional one), and classifies the outcome against the IR's top-level
//! primitives:
//!
//! - `error`: the resolution failed or panicked;
//! - `noChange`: a primitive promising an observable change (damage, life,
//!   zone change, counters, tokens, tap state, P/T) changed nothing;
//! - `wrongCount`: the change reached a different number of objects than
//!   the declared targets;
//! - `uncovered`: the rule could not be set up (no legal declaration, a
//!   non-resolution rule, a contingent program); never counted as a pass;
//! - `pass`.
//!
//! It is a robustness proof outside the fast loop: run it explicitly, in
//! release mode, over the probe's output (see
//! `docs/oracle-compositional-compiler.md`, tiers):
//!
//! ```text
//! SMOKE_INPUT=.tmp/released-vs-main/corpus-input.jsonl \
//! SMOKE_PROBE=.tmp/c6-probe/corpus-<label>.jsonl \
//! SMOKE_OUTPUT=.tmp/c6-probe/smoke-<label>.jsonl \
//! cargo test --release -p deepdeck-engine-runtime --lib runtime_smoke -- --nocapture
//! ```
//!
//! Without those variables the test returns at once, so the ordinary unit
//! runs stay fast. `SMOKE_FILTER` (comma-separated card names) narrows a run
//! to a few cards while diagnosing; a failing rule's IR is kept in the report.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};

/// Accepts optional choices and otherwise takes the first option.
struct SmokeProvider;

impl DecisionProvider for SmokeProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.options.is_empty() {
            return Err(EngineError::new("smoke decision has no option"));
        }
        // Prefer doing something: never pick a decline when another exists.
        Ok(request
            .options
            .iter()
            .position(|option| {
                option.kind != ActionKind::DeclinePayment
                    && option.kind != ActionKind::PassPriority
                    && !option.id.starts_with("decline")
                    && !option.id.ends_with(":false")
            })
            .unwrap_or(0))
    }
}

fn smoke_creature(
    id: &str,
    controller: &str,
    power: &str,
    toughness: &str,
    subtype: &str,
) -> CardInstance {
    let mut definition = test_definition(id, &format!("Creature \u{2014} {subtype}"));
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    definition.mana_cost = "{1}{G}".to_string();
    test_instance(id, definition, controller)
}

fn smoke_card(id: &str, type_line: &str, controller: &str, cost: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = cost.to_string();
    test_instance(id, definition, controller)
}

/// The synthetic board every rule is played on.
fn smoke_engine(source: Option<CardInstance>) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.step = GameStep::PrecombatMain;
    for (index, player) in ["player-0", "player-1"].iter().enumerate() {
        let tag = if index == 0 { "own" } else { "opp" };
        let mut battlefield = vec![
            smoke_creature(&format!("{tag}-bear"), player, "2", "2", "Bear"),
            smoke_creature(&format!("{tag}-giant"), player, "4", "5", "Giant"),
            smoke_card(&format!("{tag}-relic"), "Artifact", player, "{2}"),
            smoke_card(&format!("{tag}-charm"), "Enchantment", player, "{1}{W}"),
            mana_source(&format!("{tag}-forest"), "Basic Land - Forest", "{G}", None),
        ];
        // The giant is black, so color-qualified effects find a member.
        battlefield[1].definition.mana_cost = "{3}{B}".to_string();
        for card in &mut battlefield {
            card.owner = player.to_string();
            card.controller = player.to_string();
        }
        engine.state.players[index].battlefield = battlefield;
        engine.state.players[index].hand = vec![
            smoke_creature(&format!("{tag}-hand-creature"), player, "2", "2", "Elf"),
            smoke_card(&format!("{tag}-hand-instant"), "Instant", player, "{R}"),
        ];
        engine.state.players[index].graveyard = vec![
            smoke_creature(&format!("{tag}-grave-creature"), player, "3", "3", "Zombie"),
            smoke_card(&format!("{tag}-grave-sorcery"), "Sorcery", player, "{B}"),
        ];
    }
    if let Some(source) = source {
        engine.state.players[0].battlefield.push(source);
    }
    // Objects are admitted once the rule is on the stack, so the ability
    // adopts its source's incarnation (see `admit_and_resolve`).
    engine
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PermanentSnapshot {
    tapped: bool,
    damage: i32,
    counters: i32,
    power: i32,
    toughness: i32,
    controller: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Snapshot {
    life: Vec<i32>,
    hand: Vec<usize>,
    library: Vec<usize>,
    graveyard: Vec<usize>,
    exile: Vec<usize>,
    battlefield: Vec<usize>,
    permanents: BTreeMap<String, PermanentSnapshot>,
    modifiers: usize,
    poison: Vec<i32>,
}

/// Cards in a zone other than the played source: a resolved spell's own move
/// to the graveyard is not its effect.
fn others(zone: &[CardInstance]) -> usize {
    zone.iter()
        .filter(|card| card.instance_id != "smoke-source")
        .count()
}

fn snapshot(engine: &GameEngine) -> Snapshot {
    let mut permanents = BTreeMap::new();
    for player in &engine.state.players {
        for permanent in &player.battlefield {
            permanents.insert(
                permanent.instance_id.clone(),
                PermanentSnapshot {
                    tapped: permanent.tapped,
                    damage: permanent.damage_marked,
                    counters: permanent.counters.values().sum(),
                    power: engine.current_power(permanent),
                    toughness: engine.current_toughness(permanent),
                    controller: permanent.controller.clone(),
                },
            );
        }
    }
    Snapshot {
        life: engine.state.players.iter().map(|p| p.life).collect(),
        hand: engine
            .state
            .players
            .iter()
            .map(|p| others(&p.hand))
            .collect(),
        library: engine
            .state
            .players
            .iter()
            .map(|p| p.library.len())
            .collect(),
        graveyard: engine
            .state
            .players
            .iter()
            .map(|p| others(&p.graveyard))
            .collect(),
        exile: engine
            .state
            .players
            .iter()
            .map(|p| others(&p.exile))
            .collect(),
        battlefield: engine
            .state
            .players
            .iter()
            .map(|p| p.battlefield.len())
            .collect(),
        permanents,
        modifiers: engine.state.rule_modifiers.len(),
        poison: engine
            .state
            .players
            .iter()
            .map(|p| p.counters.values().sum::<i32>())
            .collect(),
    }
}

/// What changed between two snapshots, by observation kind, with the
/// permanents each kind reached.
#[derive(Debug, Default)]
struct Observed {
    kinds: BTreeSet<&'static str>,
    damaged: BTreeSet<String>,
    countered: BTreeSet<String>,
    tapped: BTreeSet<String>,
    left: BTreeSet<String>,
    stats: BTreeSet<String>,
}

fn observe(before: &Snapshot, after: &Snapshot) -> Observed {
    let mut observed = Observed::default();
    let mut mark = |condition: bool, kind: &'static str| {
        if condition {
            observed.kinds.insert(kind);
        }
    };
    mark(before.life != after.life, "life");
    mark(before.hand != after.hand, "hand");
    mark(before.library != after.library, "library");
    mark(before.graveyard != after.graveyard, "graveyard");
    mark(before.exile != after.exile, "exile");
    mark(before.battlefield != after.battlefield, "battlefield");
    mark(before.modifiers != after.modifiers, "modifiers");
    mark(before.poison != after.poison, "poison");
    for (id, old) in &before.permanents {
        match after.permanents.get(id) {
            None => {
                observed.left.insert(id.clone());
            }
            Some(new) => {
                if new.damage != old.damage {
                    observed.damaged.insert(id.clone());
                }
                if new.counters != old.counters {
                    observed.countered.insert(id.clone());
                }
                if new.tapped != old.tapped {
                    observed.tapped.insert(id.clone());
                }
                if (new.power, new.toughness) != (old.power, old.toughness) {
                    observed.stats.insert(id.clone());
                }
                if new.controller != old.controller {
                    observed.kinds.insert("control");
                }
            }
        }
    }
    let new_permanents = after
        .permanents
        .keys()
        .any(|id| !before.permanents.contains_key(id));
    if new_permanents {
        observed.kinds.insert("entered");
    }
    if !observed.left.is_empty() {
        observed.kinds.insert("left");
    }
    if !observed.damaged.is_empty() {
        observed.kinds.insert("damage");
    }
    if !observed.countered.is_empty() {
        observed.kinds.insert("counters");
    }
    if !observed.tapped.is_empty() {
        observed.kinds.insert("tapped");
    }
    if !observed.stats.is_empty() {
        observed.kinds.insert("stats");
    }
    observed
}

/// The top-level effects whose amount is zero or whose object collection is
/// empty on this board (no Swamp to count, no attacking creature): their
/// missing change says nothing about the engine. Unevaluable operands (an
/// earlier action's result) are not judged here.
fn empty_in_fixture(
    engine: &GameEngine,
    effects: &[Value],
    stack_object: &StackObject,
) -> BTreeSet<usize> {
    let bindings = BTreeMap::new();
    let mut empty = BTreeSet::new();
    for (index, effect) in effects.iter().enumerate() {
        let zero_amount = ["amount", "count", "quantity", "power", "toughness"]
            .iter()
            .filter(|field| effect[**field].is_object())
            .any(|field| {
                engine
                    .checked_runtime_numeric_expression(
                        &effect[*field],
                        stack_object,
                        &bindings,
                        &stack_object.decisions,
                        "smoke",
                    )
                    .is_ok_and(|value| value == 0)
            })
            && !(effect["power"].is_object() && effect["toughness"].is_object());
        let zero_stats = effect["power"].is_object()
            && effect["toughness"].is_object()
            && ["power", "toughness"].iter().all(|field| {
                engine
                    .checked_runtime_numeric_expression(
                        &effect[*field],
                        stack_object,
                        &bindings,
                        &stack_object.decisions,
                        "smoke",
                    )
                    .is_ok_and(|value| value == 0)
            });
        let empty_collection = ["recipient", "permanent", "object"]
            .iter()
            .filter(|slot| value_kind(&effect[**slot]) == Some("eachPermanent"))
            .any(|slot| {
                engine
                    .resolve_runtime_object_ids_checked(
                        &effect[*slot],
                        stack_object,
                        &bindings,
                        &stack_object.decisions,
                    )
                    .is_ok_and(|ids| ids.is_empty())
            });
        if zero_amount || zero_stats || empty_collection {
            empty.insert(index);
        }
    }
    empty
}

/// Changes a snapshot cannot see: a permanent that left and returned (a
/// blink keeps its instance id), life lost and gained back.
fn observe_events(observed: &mut Observed, events: &[GameEvent]) {
    for event in events {
        let id = event.card_instance_id.clone().unwrap_or_default();
        match event.kind.as_str() {
            "permanentLeftBattlefield" => {
                observed.kinds.insert("left");
                observed.left.insert(id);
            }
            // A token that entered and died to state-based actions.
            "permanentEnteredBattlefield" | "tokenCreated" => {
                observed.kinds.insert("entered");
            }
            "lifeLost" | "lifeGained" => {
                observed.kinds.insert("life");
            }
            "countersPlaced" | "countersRemoved" => {
                observed.kinds.insert("counters");
                observed.countered.insert(id);
            }
            _ => {}
        }
    }
}

/// The observation kinds a primitive promises; `None` when the primitive is
/// not checked (its effect is not observable in this snapshot).
fn promised(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "dealDamage" => &["damage", "left", "life", "poison"],
        "gainLife" | "loseLife" => &["life"],
        "drawCards" => &["hand", "library"],
        "mill" => &["library", "graveyard"],
        "discardCards" => &["hand", "graveyard"],
        "destroyPermanent" | "sacrificePermanent" | "exilePermanent" | "returnToOwnersHand" => {
            &["left", "battlefield", "graveyard", "exile", "hand"]
        }
        // A -1/-1 counter or a toughness reduction can kill its object.
        "putCounters" | "removeCounters" => &["counters", "stats", "left"],
        "createTokens" | "createToken" | "createExplicitTokens" => &["entered", "battlefield"],
        "tapPermanent" | "untapPermanent" => &["tapped"],
        "modifyPowerToughness" => &["stats", "modifiers", "left"],
        _ => return None,
    })
}

/// Top-level effects whose outcome depends on a choice, a condition or an
/// earlier result: the harness does not judge a missing change there.
fn contingent(effect: &Value) -> bool {
    if effect.is_array() {
        return effect
            .as_array()
            .is_some_and(|effects| effects.iter().any(contingent));
    }
    let kind = value_kind(effect).unwrap_or_default();
    kind.contains("onditional")
        || kind.contains("ptional")
        || kind.contains("hoose")
        || kind.contains("forEach")
        || kind.starts_with("may")
        || kind == "bind"
        || effect.get("condition").is_some()
}

/// The permanent targets a rule's declaration chose, per declaration id.
fn declared_permanent_targets(targets: &BTreeMap<String, TargetRef>, id: &str) -> usize {
    targets
        .iter()
        .filter(|(target_id, target)| {
            (*target_id == id || target_id.starts_with(&format!("{id}:")))
                && matches!(target, TargetRef::Permanent { .. })
        })
        .count()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    Pass,
    Uncovered(String),
    NoChange(String),
    WrongCount(String),
    Error(String),
}

impl Outcome {
    fn class(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Uncovered(_) => "uncovered",
            Self::NoChange(_) => "noChange",
            Self::WrongCount(_) => "wrongCount",
            Self::Error(_) => "error",
        }
    }
    fn detail(&self) -> &str {
        match self {
            Self::Pass => "",
            Self::Uncovered(detail)
            | Self::NoChange(detail)
            | Self::WrongCount(detail)
            | Self::Error(detail) => detail,
        }
    }
}

fn trigger_decisions() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("triggeringPermanentId".to_string(), json!("opp-bear")),
        (
            "triggeringPermanentController".to_string(),
            json!("player-1"),
        ),
        ("triggeringPermanentOwner".to_string(), json!("player-1")),
        ("triggeringPlayerId".to_string(), json!("player-1")),
        ("triggeringPermanentPower".to_string(), json!(2)),
        ("triggeringPermanentToughness".to_string(), json!(2)),
        ("damageAmount".to_string(), json!(2)),
    ])
}

/// Plays one rule of `card` and classifies the outcome.
fn smoke_rule(card: &CardDefinition, rule: &Value) -> (Outcome, Vec<String>) {
    let kind = value_kind(rule).unwrap_or_default().to_string();
    if !matches!(
        kind.as_str(),
        "spellAbility" | "activatedAbility" | "triggeredAbility"
    ) {
        return (Outcome::Uncovered(format!("rule:{kind}")), Vec::new());
    }
    let effects = rule["effects"].as_array().cloned().unwrap_or_default();
    let primitives = effects
        .iter()
        .filter_map(|effect| value_kind(effect).map(str::to_string))
        .collect::<Vec<_>>();
    if effects.is_empty() {
        return (Outcome::Uncovered("no effects".to_string()), primitives);
    }
    let mut source = test_instance("smoke-source", card.clone(), "player-0");
    // An Aura or Equipment source is attached to its controller's giant.
    if card
        .type_line
        .split_whitespace()
        .any(|word| matches!(word, "Aura" | "Equipment"))
    {
        source.attached_to = Some("own-giant".to_string());
    }
    let permanent_source = card.type_line.split_whitespace().any(|word| {
        matches!(
            word,
            "Creature" | "Artifact" | "Enchantment" | "Land" | "Planeswalker" | "Battle"
        )
    });
    let mut engine = smoke_engine(permanent_source.then(|| source.clone()));
    // A spell's card waits in hand so that it is admitted with the others.
    if !permanent_source {
        engine.state.players[0].hand.push(source.clone());
    }
    if let Err(error) = engine.admit_explicit_test_fixture_objects() {
        return (
            Outcome::Uncovered(format!("admission: {}", error.message)),
            primitives,
        );
    }
    let source = if permanent_source {
        let (player, index) = engine
            .permanent_position("smoke-source")
            .expect("the source is on the battlefield");
        engine.state.players[player].battlefield[index].clone()
    } else {
        let index = engine.state.players[0]
            .hand
            .iter()
            .position(|card| card.instance_id == "smoke-source")
            .expect("the spell is in hand");
        engine.state.players[0].hand.remove(index)
    };
    let mut decisions = if kind == "triggeredAbility" {
        trigger_decisions()
    } else {
        BTreeMap::new()
    };
    // A declared X (CR 107.3) is 2, so an X-sized effect is observable.
    decisions.insert("xValue".to_string(), json!(2));
    // An untap is observable only on tapped permanents.
    let effects_text = rule["effects"].to_string();
    if effects_text.contains("\"untapPermanent") && !effects_text.contains("\"tapPermanent") {
        for player in &mut engine.state.players {
            for permanent in &mut player.battlefield {
                permanent.tapped = true;
            }
        }
    }
    let declarations =
        match engine.declarations_for_triggered_rule(rule, "player-0", &source, &decisions) {
            Ok(Some(declarations)) => declarations,
            // No declaration to make: the rule resolves with no choice.
            Ok(None) if rule.get("declaration").is_none() => vec![CastingDeclaration {
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
                target_order: Vec::new(),
            }],
            Ok(None) => {
                return (
                    Outcome::Uncovered("declaration unavailable".to_string()),
                    primitives,
                );
            }
            Err(error) => {
                return (
                    Outcome::Uncovered(format!("declaration: {}", error.message)),
                    primitives,
                );
            }
        };
    // The declaration choosing the most targets exercises the rule most.
    let Some(declaration) = declarations
        .into_iter()
        .enumerate()
        .max_by_key(|(index, declaration)| (declaration.targets.len(), std::cmp::Reverse(*index)))
        .map(|(_, declaration)| declaration)
    else {
        return (
            Outcome::Uncovered("no legal declaration".to_string()),
            primitives,
        );
    };
    decisions.extend(declaration.decisions.clone());
    // The declared X stays 2 whatever the scripted declaration chose.
    decisions.insert("xValue".to_string(), json!(2));
    let targets = declaration.targets.clone();
    // A nonpermanent spell resolves as the cast card does (CR 608.2), its
    // single spell ability read from the card; anything else is an ability.
    let cast_spell = kind == "spellAbility"
        && !permanent_source
        && card
            .rules
            .iter()
            .filter(|rule| value_kind(rule) == Some("spellAbility"))
            .count()
            == 1;
    let stack_object = StackObject {
        id: "stack:smoke".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: (!cast_spell).then(|| kind.clone()),
        ability_rule: (!cast_spell).then(|| rule.clone()),
        decisions,
        targets: targets.clone(),
    };
    let empty = empty_in_fixture(&engine, &effects, &stack_object);
    engine.state.stack.push(stack_object);
    let before = snapshot(&engine);
    let first_event = engine.state.events.len();
    let resolved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.resolve_top_stack(&mut SmokeProvider)
    }));
    match resolved {
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            return (Outcome::Error(format!("panic: {message}")), primitives);
        }
        // An activation cost the harness did not pay leaves its cost
        // bindings empty: the rule was not set up, not wrong.
        Ok(Err(error))
            if error.message.contains("activationCost")
                || error.message.contains("sacrificedPermanent") =>
        {
            return (
                Outcome::Uncovered("unpaid activation cost".to_string()),
                primitives,
            );
        }
        // The synthetic event binds no observed object or spell: a trigger
        // reading one was not set up.
        Ok(Err(error))
            if kind == "triggeredAbility"
                && (error.message.contains("observed event")
                    || error.message.contains("triggering")
                    || error.message.contains("whereX")) =>
        {
            return (
                Outcome::Uncovered("synthetic trigger context".to_string()),
                primitives,
            );
        }
        Ok(Err(error)) => return (Outcome::Error(error.message), primitives),
        Ok(Ok(_)) => {}
    }
    let after = snapshot(&engine);
    if std::env::var("SMOKE_TRACE").is_ok() {
        eprintln!(
            "trace {}: targets {:?}
  before {:?}
  after {:?}",
            card.name, targets, before, after
        );
        for event in &engine.state.events[first_event..] {
            eprintln!(
                "  event {} {:?} {}",
                event.kind,
                event.card_instance_id,
                serde_json::to_string(&event.detail).unwrap_or_default()
            );
        }
    }
    let mut observed = observe(&before, &after);
    observe_events(&mut observed, &engine.state.events[first_event..]);
    // The source's own fate (a sacrifice cost, state-based actions) is not
    // the effect's change, unless the source is itself a declared target.
    let source_targeted = targets
        .values()
        .any(|target| matches!(target, TargetRef::Permanent { instance_id } if instance_id == "smoke-source"));
    if !source_targeted {
        observed.left.remove("smoke-source");
        observed.countered.remove("smoke-source");
    }
    if effects.iter().any(contingent) || rule.get("condition").is_some() {
        return (
            Outcome::Uncovered("contingent program".to_string()),
            primitives,
        );
    }
    for (index, effect) in effects.iter().enumerate() {
        let Some(kind) = value_kind(effect) else {
            continue;
        };
        let Some(expected) = promised(kind) else {
            continue;
        };
        if !expected.iter().any(|kind| observed.kinds.contains(kind)) {
            if empty.contains(&index) {
                return (
                    Outcome::Uncovered("empty amount or collection in the fixture".to_string()),
                    primitives,
                );
            }
            return (Outcome::NoChange(kind.to_string()), primitives);
        }
    }
    // The declared targets of a single-primitive rule are each reached.
    if let [effect] = effects.as_slice()
        && let Some(id) = ["recipient", "permanent", "object"]
            .iter()
            .find_map(|slot| {
                matches!(
                    value_kind(&effect[*slot]),
                    Some("chosenTarget" | "chosenTargets")
                )
                .then(|| effect[*slot]["id"].as_str())
                .flatten()
            })
    {
        let declared = declared_permanent_targets(&targets, id);
        let reached = match value_kind(effect).unwrap_or_default() {
            "dealDamage" => observed.damaged.union(&observed.left).count(),
            "putCounters" | "removeCounters" => observed.countered.union(&observed.left).count(),
            "tapPermanent" | "untapPermanent" => observed.tapped.len(),
            "destroyPermanent" | "exilePermanent" | "returnToOwnersHand" => observed.left.len(),
            _ => declared,
        };
        if declared > 0 && reached != declared {
            return (
                Outcome::WrongCount(format!(
                    "{}: {reached} of {declared} targets",
                    value_kind(effect).unwrap_or_default()
                )),
                primitives,
            );
        }
    }
    (Outcome::Pass, primitives)
}

#[derive(serde::Deserialize)]
struct SmokeRow {
    oracle_id: String,
    name: String,
    request: OracleCardParseRequest,
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn runtime_smoke() {
    let (Ok(input), Ok(probe), Ok(output)) = (
        std::env::var("SMOKE_INPUT"),
        std::env::var("SMOKE_PROBE"),
        std::env::var("SMOKE_OUTPUT"),
    ) else {
        eprintln!("runtime_smoke: set SMOKE_INPUT, SMOKE_PROBE and SMOKE_OUTPUT");
        return;
    };
    let filter = std::env::var("SMOKE_FILTER").ok().map(|names| {
        names
            .split(',')
            .map(|name| name.trim().to_string())
            .collect::<Vec<_>>()
    });
    let supported = std::io::BufReader::new(std::fs::File::open(&probe).expect("probe output"))
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|row| row["supported"] == true)
        .filter_map(|row| row["oracle_id"].as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let rows = std::io::BufReader::new(std::fs::File::open(&input).expect("corpus input"))
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<SmokeRow>(&line).ok())
        .filter(|row| supported.contains(&row.oracle_id))
        .filter(|row| {
            filter
                .as_ref()
                .is_none_or(|names| names.iter().any(|name| *name == row.name))
        })
        .collect::<Vec<_>>();
    let start = std::time::Instant::now();
    std::panic::set_hook(Box::new(|_| {}));
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&output).expect("smoke output"));
    let mut by_class = BTreeMap::<&'static str, usize>::new();
    let mut by_primitive = BTreeMap::<(String, &'static str), usize>::new();
    let mut card_class = BTreeMap::<&'static str, usize>::new();
    for row in &rows {
        let is_creature = row.request.type_line.contains("Creature");
        let compiled = std::panic::catch_unwind(|| {
            compile_playable_card(PlayableCardInput {
                id: "smoke-source".to_string(),
                face_id: None,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                power: is_creature.then(|| "2".to_string()),
                toughness: is_creature.then(|| "2".to_string()),
                oracle: row.request.clone(),
            })
        });
        let Ok(Ok(compilation)) = compiled else {
            *card_class.entry("uncovered").or_default() += 1;
            continue;
        };
        if compilation.engine_status != "executable" || !row.request.faces.is_empty() {
            *card_class.entry("uncovered").or_default() += 1;
            continue;
        }
        let mut worst = Outcome::Pass;
        let mut any_exercised = false;
        let mut rules = Vec::new();
        for rule in &compilation.card.rules {
            let (outcome, primitives) = smoke_rule(&compilation.card, rule);
            *by_class.entry(outcome.class()).or_default() += 1;
            for primitive in &primitives {
                *by_primitive
                    .entry((primitive.clone(), outcome.class()))
                    .or_default() += 1;
            }
            if !matches!(outcome, Outcome::Uncovered(_)) {
                any_exercised = true;
            }
            let mut report = json!({
                "kind": value_kind(rule),
                "class": outcome.class(),
                "detail": outcome.detail(),
                "primitives": primitives,
            });
            if matches!(
                outcome,
                Outcome::Error(_) | Outcome::NoChange(_) | Outcome::WrongCount(_)
            ) {
                report["rule"] = rule.clone();
            }
            rules.push(report);
            if outcome > worst && !matches!(outcome, Outcome::Uncovered(_)) {
                worst = outcome;
            }
        }
        let class = if matches!(worst, Outcome::Pass) && !any_exercised {
            "uncovered"
        } else {
            worst.class()
        };
        *card_class.entry(class).or_default() += 1;
        writeln!(
            writer,
            "{}",
            json!({"oracle_id": row.oracle_id, "name": row.name, "class": class, "rules": rules})
        )
        .expect("smoke output line");
    }
    let _ = std::panic::take_hook();
    eprintln!(
        "runtime_smoke: {} supported cards in {:.1}s",
        rows.len(),
        start.elapsed().as_secs_f64()
    );
    eprintln!("cards by class: {card_class:?}");
    eprintln!("rules by class: {by_class:?}");
    let mut primitives = by_primitive.into_iter().collect::<Vec<_>>();
    primitives.sort_by(|a, b| b.1.cmp(&a.1));
    for ((primitive, class), count) in primitives.iter().take(60) {
        eprintln!("  {primitive:<32} {class:<10} {count}");
    }
}
