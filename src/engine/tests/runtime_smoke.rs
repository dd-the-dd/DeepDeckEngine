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
//! runs stay fast. `SMOKE_FILTER` (card names separated by `|`) narrows a run
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
        // A priority window inside a scenario ends: the harness casts and
        // activates only through its own scripted actions.
        if request.kind == DecisionKind::Priority
            && let Some(pass) = request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
        {
            return Ok(pass);
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

/// The synthetic board every rule is played on. Each side has a green bear,
/// a black giant, a white flyer, a creature token, an artifact, an
/// enchantment, a basic and a nonbasic land; creature, instant and land
/// cards in hand; creature, sorcery and artifact cards in the graveyard;
/// a library of mixed card types.
fn smoke_engine(source: Option<CardInstance>) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.step = GameStep::PrecombatMain;
    for (index, player) in ["player-0", "player-1"].iter().enumerate() {
        let tag = if index == 0 { "own" } else { "opp" };
        let mut flyer = smoke_creature(&format!("{tag}-flyer"), player, "1", "1", "Bird");
        flyer.definition.mana_cost = "{W}".to_string();
        flyer.definition.rules = vec![json!({
            "kind": "keywordAbility",
            "ability": { "kind": "flying" },
        })];
        let mut token = smoke_creature(&format!("{tag}-token"), player, "1", "1", "Soldier");
        token.definition.is_token = true;
        token.definition.mana_cost = String::new();
        let mut battlefield = vec![
            smoke_creature(&format!("{tag}-bear"), player, "2", "2", "Bear"),
            smoke_creature(&format!("{tag}-giant"), player, "4", "5", "Giant"),
            flyer,
            token,
            smoke_card(&format!("{tag}-relic"), "Artifact", player, "{2}"),
            smoke_card(&format!("{tag}-charm"), "Enchantment", player, "{1}{W}"),
            mana_source(&format!("{tag}-forest"), "Basic Land - Forest", "{G}", None),
            smoke_card(&format!("{tag}-mine"), "Land", player, ""),
        ];
        // The giant is black, so color-qualified effects find a member.
        battlefield[1].definition.mana_cost = "{3}{B}".to_string();
        for card in &mut battlefield {
            card.owner = player.to_string();
            card.controller = player.to_string();
            card.summoning_sick = false;
        }
        engine.state.players[index].battlefield = battlefield;
        // The instant is a castable spell (`you gain 1 life`), for cast
        // triggers and for a spell to counter.
        let mut instant = smoke_card(&format!("{tag}-hand-instant"), "Instant", player, "{R}");
        instant.definition.rules = vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "gainLife",
                "amount": { "kind": "integer", "value": 1 },
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            }],
        })];
        engine.state.players[index].hand = vec![
            smoke_creature(&format!("{tag}-hand-creature"), player, "2", "2", "Elf"),
            instant,
            smoke_card(
                &format!("{tag}-hand-land"),
                "Basic Land - Forest",
                player,
                "",
            ),
        ];
        engine.state.players[index].graveyard = vec![
            smoke_creature(&format!("{tag}-grave-creature"), player, "3", "3", "Zombie"),
            smoke_card(&format!("{tag}-grave-sorcery"), "Sorcery", player, "{B}"),
            smoke_card(&format!("{tag}-grave-artifact"), "Artifact", player, "{1}"),
        ];
        engine.state.players[index].library = vec![
            smoke_creature(&format!("{tag}-library-creature"), player, "2", "2", "Elf"),
            smoke_card(
                &format!("{tag}-library-land"),
                "Basic Land - Forest",
                player,
                "",
            ),
            smoke_card(&format!("{tag}-library-instant"), "Instant", player, "{U}"),
            smoke_creature(&format!("{tag}-library-beast"), player, "3", "3", "Beast"),
            smoke_card(
                &format!("{tag}-library-artifact"),
                "Artifact",
                player,
                "{1}",
            ),
            smoke_card(&format!("{tag}-library-sorcery"), "Sorcery", player, "{B}"),
        ];
    }
    if let Some(source) = source {
        engine.state.players[0].battlefield.push(source);
    }
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

/// Context the synthetic board does not set up for an effect: a combat
/// counterpart, an observed event object or spell, a paid cost's value, an
/// earlier action's result, a declared slot left empty (`up to one`), a
/// spell acting on its own card.
fn unset_context(
    effect: &Value,
    targets: &BTreeMap<String, TargetRef>,
    permanent_source: bool,
) -> Option<&'static str> {
    let text = effect.to_string();
    for (marker, context) in [
        ("\"combatCounterpart\"", "no combat counterpart"),
        ("\"observedEventObject\"", "no observed event object"),
        ("\"triggeringSpell", "no triggering spell"),
        ("\"sacrificedPermanent", "unpaid sacrifice cost"),
        ("\"boundObjects\"", "an earlier action's result"),
    ] {
        if text.contains(marker) {
            return Some(context);
        }
    }
    for slot in ["recipient", "permanent", "object", "cards"] {
        let operand = &effect[slot];
        match value_kind(operand) {
            Some("chosenTarget" | "chosenTargets") => {
                let id = operand["id"].as_str().unwrap_or_default();
                let declared = targets
                    .keys()
                    .any(|target_id| target_id == id || target_id.starts_with(&format!("{id}:")));
                if !declared {
                    return Some("no target chosen for an optional slot");
                }
            }
            Some("self") if !permanent_source => return Some("a spell acting on its own card"),
            _ => {}
        }
    }
    None
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
        let absent_counter = value_kind(effect) == Some("removeCounters")
            && value_kind(&effect["permanent"]) == Some("self")
            && effect["counter"].as_str().is_some_and(|counter| {
                engine
                    .permanent_position(&stack_object.card.instance_id)
                    .is_none_or(|(player, index)| {
                        engine.state.players[player].battlefield[index]
                            .counters
                            .get(counter)
                            .copied()
                            .unwrap_or(0)
                            == 0
                    })
            });
        // A tap of a target the cost already tapped (`Tap an untapped
        // creature you control: tap target creature`).
        let already_tapped = value_kind(effect) == Some("tapPermanent")
            && matches!(
                value_kind(&effect["permanent"]),
                Some("chosenTarget" | "chosenTargets")
            )
            && stack_object.targets.values().all(|target| match target {
                TargetRef::Permanent { instance_id } => engine
                    .permanent_position(instance_id)
                    .is_some_and(|(player, index)| {
                        engine.state.players[player].battlefield[index].tapped
                    }),
                _ => false,
            });
        if zero_amount || zero_stats || empty_collection || absent_counter || already_tapped {
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
            // A permanent tapped, then destroyed by the same rule.
            "permanentTapped" => {
                observed.kinds.insert("tapped");
                observed.tapped.insert(id);
            }
            // A discard after a return to hand leaves the hand size unchanged.
            "cardDiscarded" => {
                observed.kinds.insert("hand");
            }
            "lifeLost" | "lifeGained" => {
                observed.kinds.insert("life");
            }
            "damageDealt" => {
                if id.is_empty() {
                    observed.kinds.insert("life");
                } else {
                    observed.kinds.insert("damage");
                    observed.damaged.insert(id);
                }
            }
            // Damage prevented (protection) still reached its recipient.
            "damagePrevented" => {
                observed.kinds.insert("damage");
                observed.damaged.insert(id);
            }
            "powerToughnessModified" => {
                observed.kinds.insert("stats");
                observed.stats.insert(id);
            }
            "permanentUntapped" => {
                observed.kinds.insert("tapped");
                observed.tapped.insert(id);
            }
            "cardDrawn" => {
                observed.kinds.insert("hand");
            }
            // A zone move of a card other than the played source.
            "cardsMoved" | "cardExiled" | "cardsEnteredGraveyard" => {
                let moved = event.detail["cardIds"]
                    .as_array()
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![id.clone()]);
                if moved
                    .iter()
                    .any(|card| !card.is_empty() && card != "smoke-source")
                {
                    for kind in ["graveyard", "exile", "hand", "library", "battlefield"] {
                        observed.kinds.insert(kind);
                    }
                }
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
    /// A contingent program (a choice, a condition) of which some primitive
    /// changed an object other than its source: weaker than `Pass`.
    ContingentPass,
    Uncovered(String),
    /// The real triggering event happened; the source's trigger did not.
    NoTrigger(String),
    NoChange(String),
    WrongCount(String),
    Error(String),
}

impl Outcome {
    fn class(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::ContingentPass => "contingentPass",
            Self::Uncovered(_) => "uncovered",
            Self::NoTrigger(_) => "noTrigger",
            Self::NoChange(_) => "noChange",
            Self::WrongCount(_) => "wrongCount",
            Self::Error(_) => "error",
        }
    }
    fn detail(&self) -> &str {
        match self {
            Self::Pass | Self::ContingentPass => "",
            Self::Uncovered(detail)
            | Self::NoTrigger(detail)
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

/// A player's floating mana for the costs the scenario will pay: each
/// colored symbol of `costs`, their generic amount, and four more (X = 2,
/// a fixture spell) of colorless mana. Sizing the pool to the costs keeps
/// the engine's payment search, and the X values it offers, small.
fn fill_mana_pool(engine: &mut GameEngine, player: usize, costs: &[&str]) {
    let mut symbols = Vec::new();
    let mut generic = 4;
    for cost in costs {
        for symbol in cost.split(['{', '}']).filter(|symbol| !symbol.is_empty()) {
            if let Ok(amount) = symbol.parse::<usize>() {
                generic += amount;
                continue;
            }
            for part in symbol.split('/') {
                if matches!(part, "W" | "U" | "B" | "R" | "G" | "C") {
                    symbols.push(part.to_string());
                } else if part == "2" {
                    generic += 2;
                }
            }
        }
    }
    symbols.extend(std::iter::repeat_n("C".to_string(), generic));
    for symbol in symbols {
        engine.state.players[player].mana_pool.push(FloatingMana {
            symbol,
            spend_restriction: None,
        });
    }
}

/// The mana costs a rule's scenario pays: the card's, its activation's, and
/// those of the fixture spells an event scenario casts.
fn scenario_costs(card: &CardDefinition, rule: &Value) -> Vec<String> {
    let mut costs = vec![
        card.mana_cost.clone(),
        "{1}{G}".to_string(),
        "{R}".to_string(),
    ];
    for cost in rule["costs"].as_array().into_iter().flatten() {
        if let Some(mana) = cost["manaCost"].as_str() {
            costs.push(mana.to_string());
        }
    }
    costs
}

/// How the rule reached the stack: through the real cast or activation
/// (costs paid by the engine), its real triggering event, a real combat,
/// or the synthetic declaration fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Path {
    Cast,
    Activate,
    Event,
    Combat,
    Synthetic,
}

impl Path {
    fn name(self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::Activate => "activate",
            Self::Event => "event",
            Self::Combat => "combat",
            Self::Synthetic => "synthetic",
        }
    }
}

/// The legal action of `player` that casts (or activates rule `rule_index`
/// of) `card_id`: the one declaring X = 2 when X is declared, then the one
/// with the most targets.
fn real_action(
    engine: &GameEngine,
    player: usize,
    action_kind: ActionKind,
    card_id: &str,
    rule_index: Option<usize>,
) -> Result<Option<LegalAction>, EngineError> {
    Ok(engine
        .try_legal_priority_actions(player)?
        .into_iter()
        .filter(|action| action.kind == action_kind)
        .filter(|action| action.card_instance_id.as_deref() == Some(card_id))
        .filter(|action| match rule_index {
            Some(index) => {
                action
                    .decisions
                    .get("abilityRuleIndex")
                    .and_then(Value::as_u64)
                    == Some(index as u64)
            }
            // Alternative casts (flashback, foretell, …) are other rules.
            None => !action.decisions.contains_key("abilityRuleIndex"),
        })
        .enumerate()
        .max_by_key(|(index, action)| {
            // A cost that sacrifices, returns or discards a declared target
            // makes the rule fizzle: prefer a payment that spares targets.
            let decisions = serde_json::to_string(&action.decisions).unwrap_or_default();
            let spares_targets = action.targets.values().all(|target| match target {
                TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                    !decisions.contains(&format!("\"{instance_id}\""))
                }
                _ => true,
            });
            (
                spares_targets,
                action.decisions.get("xValue").and_then(Value::as_i64) == Some(2),
                action.targets.len(),
                std::cmp::Reverse(*index),
            )
        })
        .map(|(_, action)| action))
}

fn event_text(rule: &Value) -> String {
    rule["event"].to_string()
}

fn is_combat_event(rule: &Value) -> bool {
    let text = event_text(rule);
    [
        "\"declaredAttacker\"",
        "\"controlledCreaturesAttacked\"",
        "\"attackDeclared\"",
        "\"block\"",
        "\"combatDamageToPlayer\"",
        "\"combatDamage",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// Declares the scripted attack and block, then passes priority.
struct CombatProvider {
    attacker: String,
    blocker: Option<String>,
}

impl DecisionProvider for CombatProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let position = |kind: ActionKind, matches: &dyn Fn(&LegalAction) -> bool| {
            request
                .options
                .iter()
                .position(|action| action.kind == kind && matches(action))
        };
        let chosen = match request.kind {
            DecisionKind::Attackers => position(ActionKind::DeclareAttacker, &|action| {
                action.attacker_id.as_deref() == Some(self.attacker.as_str())
                    && action
                        .targets
                        .values()
                        .all(|target| !matches!(target, TargetRef::Permanent { .. }))
            })
            .or_else(|| {
                position(ActionKind::DeclareAttacker, &|action| {
                    action.attacker_id.as_deref() == Some(self.attacker.as_str())
                })
            })
            .or_else(|| position(ActionKind::FinishAttackers, &|_| true)),
            DecisionKind::Blockers => self
                .blocker
                .as_deref()
                .and_then(|blocker| {
                    position(ActionKind::DeclareBlocker, &|action| {
                        action.attacker_id.as_deref() == Some(self.attacker.as_str())
                            && action.blocker_id.as_deref() == Some(blocker)
                    })
                })
                .or_else(|| position(ActionKind::FinishBlockers, &|_| true)),
            DecisionKind::Priority => position(ActionKind::PassPriority, &|_| true),
            _ => None,
        };
        match chosen {
            Some(index) => Ok(index),
            None => SmokeProvider.choose(state, request),
        }
    }
}

/// The position of the smoke source's stack object for `rule`.
fn rule_on_stack(engine: &GameEngine, rule: &Value) -> Option<usize> {
    engine.state.stack.iter().rposition(|object| {
        object.card.instance_id == "smoke-source" && object.ability_rule.as_ref() == Some(rule)
    })
}

/// The fixture object an event scenario acts with: the entering permanent,
/// the cast spell, the creature that dies.
fn scenario_object(rule: &Value) -> Option<&'static str> {
    let event = &rule["event"];
    let names_opponent = event.to_string().contains("pponent");
    match value_kind(event)? {
        "permanentEntered" if event["where"].to_string().contains("\"Land\"") => {
            Some("own-hand-land")
        }
        "permanentEntered" => Some("own-hand-creature"),
        "spellCast" if names_opponent => Some("opp-hand-instant"),
        "spellCast" => Some("own-hand-instant"),
        "permanentDied" if value_kind(&event["object"]) == Some("self") => Some("smoke-source"),
        "permanentDied" if names_opponent => Some("opp-bear"),
        "permanentDied" => Some("own-bear"),
        _ => None,
    }
}

/// Whether the scenario's event meets every filter the rule's event names:
/// only the event kind, its player relation and a `where` the scenario
/// object satisfies. A trigger that does not fire on an event that misses
/// a filter is not a defect.
fn scenario_meets_event_filters(engine: &GameEngine, rule: &Value, card: &CardDefinition) -> bool {
    let event = &rule["event"];
    let Some(fields) = event.as_object() else {
        return false;
    };
    let plain = fields.iter().all(|(key, value)| {
        matches!(
            key.as_str(),
            "kind"
                | "player"
                | "object"
                | "anyPlayer"
                | "anyController"
                | "opponentOfSourceController"
        ) || (key == "where" && value.is_null())
            || (key == "where" && !value.is_null())
    });
    if !plain {
        return false;
    }
    let Some(predicate) = event.get("where").filter(|predicate| !predicate.is_null()) else {
        return true;
    };
    let Some(object) = scenario_object(rule) else {
        return false;
    };
    let definition = if object == "smoke-source" {
        Some(card.clone())
    } else {
        engine
            .state
            .players
            .iter()
            .flat_map(|player| {
                player
                    .battlefield
                    .iter()
                    .chain(&player.graveyard)
                    .chain(&player.hand)
                    .chain(&player.exile)
            })
            .chain(engine.state.stack.iter().map(|object| &object.card))
            .find(|candidate| candidate.instance_id == object)
            .map(|candidate| candidate.definition.clone())
    };
    definition.is_some_and(|definition| matches_card_filter(&definition, predicate))
}

/// Produces the rule's real triggering event. `Ok(None)`: the harness has
/// no scenario for this event (the synthetic fallback applies).
fn fire_event(
    engine: &mut GameEngine,
    rule: &Value,
    card: &CardDefinition,
) -> Result<Option<Path>, EngineError> {
    let event = &rule["event"];
    let event_kind = value_kind(event).unwrap_or_default();
    let object_is_self = value_kind(&event["object"]) == Some("self");
    let is_creature = card.type_line.contains("Creature");
    let names_opponent = event.to_string().contains("pponent");
    match event_kind {
        // The source enters: it is cast from hand (or played, a land).
        "enterBattlefield" if object_is_self => {
            let land = card.type_line.contains("Land");
            let kind = if land {
                ActionKind::PlayLand
            } else {
                ActionKind::CastSpell
            };
            let Some(action) = real_action(engine, 0, kind, "smoke-source", None)? else {
                return Ok(None);
            };
            engine.apply_priority_action(&action, &mut SmokeProvider)?;
            if !land {
                engine.resolve_top_stack(&mut SmokeProvider)?;
            }
            Ok(Some(Path::Event))
        }
        // Another creature enters under its controller's control.
        "permanentEntered" if !names_opponent => {
            let entering = scenario_object(rule).unwrap_or("own-hand-creature");
            let kind = if entering == "own-hand-land" {
                ActionKind::PlayLand
            } else {
                ActionKind::CastSpell
            };
            let Some(action) = real_action(engine, 0, kind.clone(), entering, None)? else {
                return Ok(None);
            };
            engine.apply_priority_action(&action, &mut SmokeProvider)?;
            if kind == ActionKind::CastSpell {
                engine.resolve_top_stack(&mut SmokeProvider)?;
            }
            Ok(Some(Path::Event))
        }
        // The source, or another creature, dies of lethal damage.
        "permanentDied" => {
            let victim = if object_is_self {
                if !is_creature {
                    return Ok(None);
                }
                "smoke-source"
            } else if names_opponent {
                "opp-bear"
            } else {
                "own-bear"
            };
            let (player, index) = engine
                .permanent_position(victim)
                .ok_or_else(|| EngineError::new("smoke victim is absent"))?;
            let lethal = engine
                .current_toughness(&engine.state.players[player].battlefield[index])
                .max(1);
            engine.state.players[player].battlefield[index].damage_marked = lethal;
            engine.check_state_based_actions_with_provider(Some(&mut SmokeProvider))?;
            Ok(Some(Path::Event))
        }
        // A step begins for the source's controller (or an opponent).
        "stepBegan" => {
            let Some(step) = event["step"].as_str() else {
                return Ok(None);
            };
            engine.state.active_player = usize::from(names_opponent);
            engine.enqueue_step_triggers(step);
            Ok(Some(Path::Event))
        }
        // Its controller (or an opponent) casts an instant.
        "spellCast" => {
            let (player, spell) = if names_opponent {
                (1, "opp-hand-instant")
            } else {
                (0, "own-hand-instant")
            };
            if names_opponent {
                fill_mana_pool(engine, 1, &["{R}"]);
            }
            let Some(action) = real_action(engine, player, ActionKind::CastSpell, spell, None)?
            else {
                return Ok(None);
            };
            engine.apply_priority_action(&action, &mut SmokeProvider)?;
            Ok(Some(Path::Event))
        }
        _ if is_combat_event(rule) => {
            if !is_creature {
                return Ok(None);
            }
            // The source attacks and the opponent's bear blocks when the
            // event names a block; the source blocks when it is the blocker.
            let text = event_text(rule);
            let source_blocks =
                text.contains("\"subject\":{\"object\":{\"kind\":\"self\"},\"role\":\"blocker\"");
            let blocks = text.contains("\"block\"");
            let mut provider = if source_blocks {
                engine.state.active_player = 1;
                CombatProvider {
                    attacker: "opp-bear".to_string(),
                    blocker: Some("smoke-source".to_string()),
                }
            } else {
                CombatProvider {
                    attacker: "smoke-source".to_string(),
                    blocker: blocks.then(|| "opp-bear".to_string()),
                }
            };
            engine.run_combat(&mut provider)?;
            Ok(Some(Path::Combat))
        }
        _ => Ok(None),
    }
}

/// Observations from the events of one stack object's resolution, with the
/// state-based actions that follow it.
fn observe_window(events: &[GameEvent], stack_id: &str) -> Option<Observed> {
    let start = events.iter().position(|event| {
        event.kind == "stackObjectStartedResolving" && event.detail["stackId"] == stack_id
    })?;
    let end = events[start..]
        .iter()
        .position(|event| event.kind.ends_with("Resolved") && event.detail["stackId"] == stack_id)
        .map(|offset| start + offset)?;
    let tail = events[end + 1..]
        .iter()
        .position(|event| event.kind == "stackObjectStartedResolving")
        .map_or(events.len(), |offset| end + 1 + offset);
    let mut observed = Observed::default();
    observe_events(&mut observed, &events[start..tail]);
    Some(observed)
}

/// The objects a collection effect must change: the shared object query's
/// answer before resolution (the scope the collection names).
fn expected_collection(
    engine: &GameEngine,
    effect: &Value,
    stack_object: &StackObject,
) -> Option<(&'static str, BTreeSet<String>)> {
    let slot = ["recipient", "permanent", "object"]
        .into_iter()
        .find(|slot| value_kind(&effect[*slot]) == Some("eachPermanent"))?;
    let ids = engine
        .resolve_runtime_object_ids_checked(
            &effect[slot],
            stack_object,
            &BTreeMap::new(),
            &stack_object.decisions,
        )
        .ok()?;
    let tapped = |id: &String| {
        engine
            .permanent_position(id)
            .is_some_and(|(player, index)| engine.state.players[player].battlefield[index].tapped)
    };
    let ids = match value_kind(effect) {
        // A tap or untap changes only the permanents not already so.
        Some("tapPermanent") => ids.into_iter().filter(|id| !tapped(id)).collect(),
        Some("untapPermanent") => ids.into_iter().filter(|id| tapped(id)).collect(),
        Some(
            "dealDamage" | "destroyPermanent" | "exilePermanent" | "returnToOwnersHand"
            | "putCounters",
        ) => ids.into_iter().collect(),
        _ => return None,
    };
    Some((slot, ids))
}

fn reached_collection(effect: &Value, observed: &Observed) -> BTreeSet<String> {
    match value_kind(effect).unwrap_or_default() {
        "dealDamage" => observed.damaged.union(&observed.left).cloned().collect(),
        "putCounters" => observed.countered.union(&observed.left).cloned().collect(),
        "tapPermanent" | "untapPermanent" => observed.tapped.clone(),
        _ => observed.left.clone(),
    }
}

/// `another target`: a later target slot that excludes its source must not
/// name a permanent an earlier slot of the same declaration names.
fn reused_another_target(rule: &Value, targets: &BTreeMap<String, TargetRef>) -> Option<String> {
    let decisions = rule["declaration"]["decisions"].as_array()?;
    let slot_of = |target_id: &str| target_id.split(':').next().unwrap_or_default().to_string();
    for (later_index, later) in decisions.iter().enumerate() {
        if value_kind(later) != Some("chooseTargets")
            || later["candidates"]["excludeSource"].as_bool() != Some(true)
        {
            continue;
        }
        let later_id = later["id"].as_str().unwrap_or_default();
        for earlier in &decisions[..later_index] {
            let earlier_id = earlier["id"].as_str().unwrap_or_default();
            let reused = targets.iter().any(|(target_id, target)| {
                slot_of(target_id) == later_id
                    && matches!(target, TargetRef::Permanent { .. })
                    && targets
                        .iter()
                        .any(|(other_id, other)| slot_of(other_id) == earlier_id && other == target)
            });
            if reused {
                return Some(format!("{earlier_id} and {later_id} name the same object"));
            }
        }
    }
    None
}

/// Plays one rule of `card` and classifies the outcome.
fn smoke_rule(
    card: &CardDefinition,
    rule: &Value,
    rule_index: usize,
) -> (Outcome, Vec<String>, Path) {
    let kind = value_kind(rule).unwrap_or_default().to_string();
    if !matches!(
        kind.as_str(),
        "spellAbility" | "activatedAbility" | "triggeredAbility"
    ) {
        return (
            Outcome::Uncovered(format!("rule:{kind}")),
            Vec::new(),
            Path::Synthetic,
        );
    }
    let effects = rule["effects"].as_array().cloned().unwrap_or_default();
    let primitives = effects
        .iter()
        .filter_map(|effect| value_kind(effect).map(str::to_string))
        .collect::<Vec<_>>();
    if effects.is_empty() {
        return (
            Outcome::Uncovered("no effects".to_string()),
            primitives,
            Path::Synthetic,
        );
    }
    // A Vanguard is neither cast nor put onto the battlefield.
    if card.type_line.contains("Vanguard") {
        return (
            Outcome::Uncovered("vanguard".to_string()),
            primitives,
            Path::Synthetic,
        );
    }
    // A cast instant or sorcery resolves all its spell abilities at once:
    // they are judged together, on the first one.
    let permanent_card = card.type_line.split_whitespace().any(|word| {
        matches!(
            word,
            "Creature" | "Artifact" | "Enchantment" | "Land" | "Planeswalker" | "Battle"
        )
    });
    let spell_rules = card
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| value_kind(rule) == Some("spellAbility"))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let effects = if kind == "spellAbility" && !permanent_card && spell_rules.len() > 1 {
        if spell_rules.first() != Some(&rule_index) {
            return (
                Outcome::Uncovered("judged with the card's first spell ability".to_string()),
                primitives,
                Path::Cast,
            );
        }
        spell_rules
            .iter()
            .flat_map(|index| {
                card.rules[*index]["effects"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
    } else {
        effects
    };
    let (outcome, path) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        play_rule(card, rule, rule_index, &kind, &effects)
    })) {
        Ok(result) => result,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            (Outcome::Error(format!("panic: {message}")), Path::Synthetic)
        }
    };
    (outcome, primitives, path)
}

fn play_rule(
    card: &CardDefinition,
    rule: &Value,
    rule_index: usize,
    kind: &str,
    effects: &[Value],
) -> (Outcome, Path) {
    let uncovered = |reason: &str, path: Path| (Outcome::Uncovered(reason.to_string()), path);
    let permanent_source = card.type_line.split_whitespace().any(|word| {
        matches!(
            word,
            "Creature" | "Artifact" | "Enchantment" | "Land" | "Planeswalker" | "Battle"
        )
    });
    let mut source = test_instance("smoke-source", card.clone(), "player-0");
    // An Aura or Equipment source is attached to its controller's giant.
    if card
        .type_line
        .split_whitespace()
        .any(|word| matches!(word, "Aura" | "Equipment"))
    {
        source.attached_to = Some("own-giant".to_string());
    }
    if card.type_line.contains("Planeswalker") {
        source.counters.insert("loyalty".to_string(), 5);
    }
    // The source has been under its controller's control since the turn
    // began: it can pay a {T} cost.
    source.summoning_sick = false;
    let enters_from_hand = kind == "triggeredAbility"
        && value_kind(&rule["event"]) == Some("enterBattlefield")
        && value_kind(&rule["event"]["object"]) == Some("self");
    let on_battlefield = permanent_source && !enters_from_hand;
    let mut engine = smoke_engine(on_battlefield.then(|| source.clone()));
    if !on_battlefield {
        engine.state.players[0].hand.push(source.clone());
    }
    // A rule that targets a spell finds the opponent's instant on the stack.
    let rule_text = rule.to_string();
    if rule_text.contains("\"counterSpell\"") || rule_text.contains("\"spells\"") {
        let index = engine.state.players[1]
            .hand
            .iter()
            .position(|card| card.instance_id == "opp-hand-instant")
            .expect("the opponent's instant");
        let spell = engine.state.players[1].hand.remove(index);
        engine.state.stack.push(StackObject {
            id: "stack:opponent".to_string(),
            controller: "player-1".to_string(),
            card: spell,
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: None,
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        });
    }
    let costs = scenario_costs(card, rule);
    fill_mana_pool(
        &mut engine,
        0,
        &costs.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    // An untap is observable only on tapped permanents.
    let effects_text = rule["effects"].to_string();
    if effects_text.contains("\"untapPermanent") && !effects_text.contains("\"tapPermanent") {
        for player in &mut engine.state.players {
            for permanent in &mut player.battlefield {
                // The source too, unless tapping it pays the activation.
                let pays_with_tap = rule["costs"]
                    .as_array()
                    .is_some_and(|costs| costs.iter().any(|cost| value_kind(cost) == Some("tap")));
                if permanent.instance_id != "smoke-source" || !pays_with_tap {
                    permanent.tapped = true;
                }
            }
        }
    }
    if let Err(error) = engine.admit_explicit_test_fixture_objects() {
        return uncovered(&format!("admission: {}", error.message), Path::Synthetic);
    }
    // The real path, when there is one: the engine pays the costs and
    // makes the declaration.
    let (real, mut path) = match kind {
        "spellAbility" if !permanent_source => (
            real_action(&engine, 0, ActionKind::CastSpell, "smoke-source", None),
            Path::Cast,
        ),
        "activatedAbility" => (
            real_action(
                &engine,
                0,
                ActionKind::ActivateAbility,
                "smoke-source",
                Some(rule_index),
            ),
            Path::Activate,
        ),
        _ => (Ok(None), Path::Synthetic),
    };
    let real = match real {
        Ok(real) => real,
        Err(error) => return uncovered(&format!("legal actions: {}", error.message), path),
    };
    let mut combat_window = None;
    if let Some(action) = &real {
        if let Some(reused) = reused_another_target(rule, &action.targets) {
            return (
                Outcome::WrongCount(format!("another target: {reused}")),
                path,
            );
        }
        if let Err(error) = engine.apply_priority_action(action, &mut SmokeProvider) {
            return (
                Outcome::Error(format!("{}: {}", path.name(), error.message)),
                path,
            );
        }
    } else if kind == "triggeredAbility" {
        if rule.get("condition").is_some() {
            return uncovered("intervening condition", Path::Synthetic);
        }
        let first_event = engine.state.events.len();
        match fire_event(&mut engine, rule, card) {
            Ok(Some(fired)) => {
                path = fired;
                if fired == Path::Combat {
                    combat_window = Some(first_event);
                } else if rule_on_stack(&engine, rule).is_none() {
                    let triggered = engine.state.events[first_event..].iter().any(|event| {
                        event.kind == "triggeredAbilityPutOnStack"
                            && event.card_instance_id.as_deref() == Some("smoke-source")
                    });
                    if !triggered {
                        if !scenario_meets_event_filters(&engine, rule, card) {
                            return uncovered(
                                "the scenario event misses the event's filters",
                                path,
                            );
                        }
                        return (Outcome::NoTrigger(event_text(rule)), path);
                    }
                    return uncovered("trigger left the stack (no legal target)", path);
                }
            }
            Ok(None) => {}
            Err(error) => {
                return uncovered(&format!("event scenario: {}", error.message), Path::Event);
            }
        }
    } else if kind == "activatedAbility" {
        return uncovered("not activatable on the fixture", path);
    } else if !permanent_source {
        return uncovered("not castable on the fixture", path);
    }
    // A combat trigger resolved during combat: it is judged on the events
    // of its resolution window.
    if let Some(first_event) = combat_window {
        let events = &engine.state.events[first_event..];
        if std::env::var("SMOKE_TRACE").is_ok() {
            eprintln!("trace {} (combat)", card.name);
            for event in events {
                eprintln!(
                    "  event {} {:?} {}",
                    event.kind,
                    event.card_instance_id,
                    serde_json::to_string(&event.detail).unwrap_or_default()
                );
            }
        }
        let Some(stack_id) = events
            .iter()
            .find(|event| {
                event.kind == "triggeredAbilityPutOnStack"
                    && event.card_instance_id.as_deref() == Some("smoke-source")
            })
            .and_then(|event| event.detail["stackId"].as_str())
            .map(str::to_string)
        else {
            // Only an unfiltered event about the source itself is fully set
            // up by the scripted combat (a counterpart filter may not match
            // the fixture bear).
            let plain = rule["event"].as_object().is_some_and(|fields| {
                fields
                    .keys()
                    .all(|key| matches!(key.as_str(), "kind" | "object" | "source"))
            });
            if !plain {
                return uncovered("the scenario combat misses the event's filters", path);
            }
            if !events.iter().any(|event| event.kind == "attackDeclared") {
                return uncovered("the scripted attack was not declared", path);
            }
            return (Outcome::NoTrigger(event_text(rule)), path);
        };
        let Some(observed) = observe_window(events, &stack_id) else {
            return uncovered("combat trigger did not resolve", path);
        };
        return judge_window(effects, &observed, rule, path);
    }
    let synthetic = path == Path::Synthetic;
    if synthetic {
        let mut decisions = if kind == "triggeredAbility" {
            trigger_decisions()
        } else {
            BTreeMap::new()
        };
        decisions.insert("xValue".to_string(), json!(2));
        // No real path: the declaration is made for the rule directly.
        let source = if on_battlefield {
            let (player, index) = engine
                .permanent_position("smoke-source")
                .expect("the source is on the battlefield");
            engine.state.players[player].battlefield[index].clone()
        } else {
            let index = engine.state.players[0]
                .hand
                .iter()
                .position(|card| card.instance_id == "smoke-source")
                .expect("the source is in hand");
            engine.state.players[0].hand.remove(index)
        };
        let declarations =
            match engine.declarations_for_triggered_rule(rule, "player-0", &source, &decisions) {
                Ok(Some(declarations)) => declarations,
                Ok(None) if rule.get("declaration").is_none() => vec![CastingDeclaration {
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                }],
                Ok(None) => return uncovered("declaration unavailable", path),
                Err(error) => return uncovered(&format!("declaration: {}", error.message), path),
            };
        let Some(declaration) = declarations
            .into_iter()
            .enumerate()
            .max_by_key(|(index, declaration)| {
                (declaration.targets.len(), std::cmp::Reverse(*index))
            })
            .map(|(_, declaration)| declaration)
        else {
            return uncovered("no legal declaration", path);
        };
        decisions.extend(declaration.decisions.clone());
        decisions.insert("xValue".to_string(), json!(2));
        engine.state.stack.push(StackObject {
            id: "stack:smoke".to_string(),
            controller: "player-0".to_string(),
            card: source,
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some(kind.to_string()),
            ability_rule: Some(rule.clone()),
            decisions,
            targets: declaration.targets.clone(),
        });
    } else if kind == "triggeredAbility" {
        // Resolve what stacked above the trigger, then commit its targets.
        let mut resolved_above = 0;
        while let Some(position) = rule_on_stack(&engine, rule) {
            if position + 1 == engine.state.stack.len() {
                break;
            }
            resolved_above += 1;
            if resolved_above > 16 {
                return uncovered(
                    "event scenario: the stack above the trigger does not settle",
                    path,
                );
            }
            if let Err(error) = engine.resolve_top_stack(&mut SmokeProvider) {
                return uncovered(&format!("event scenario: {}", error.message), path);
            }
        }
        if let Err(error) = engine.commit_pending_triggered_targets(&mut SmokeProvider) {
            return (
                Outcome::Error(format!("trigger targets: {}", error.message)),
                path,
            );
        }
        if rule_on_stack(&engine, rule).map(|position| position + 1)
            != Some(engine.state.stack.len())
        {
            return uncovered("trigger left the stack (no legal target)", path);
        }
    }
    let Some(stack_object) = engine.state.stack.last().cloned() else {
        return uncovered("nothing on the stack", path);
    };
    if !synthetic && let Some(reused) = reused_another_target(rule, &stack_object.targets) {
        return (
            Outcome::WrongCount(format!("another target: {reused}")),
            path,
        );
    }
    let targets = stack_object.targets.clone();
    let empty = empty_in_fixture(&engine, effects, &stack_object);
    let collections = effects
        .iter()
        .map(|effect| expected_collection(&engine, effect, &stack_object))
        .collect::<Vec<_>>();
    let before = snapshot(&engine);
    let first_event = engine.state.events.len();
    match engine.resolve_top_stack(&mut SmokeProvider) {
        Ok(()) => {}
        Err(error)
            if error.message.contains("activationCost")
                || error.message.contains("sacrificedPermanent")
                || rule["effects"]
                    .to_string()
                    .contains("\"sacrificedPermanent") =>
        {
            return uncovered("unpaid activation cost", path);
        }
        Err(error)
            if synthetic
                && kind == "triggeredAbility"
                && (error.message.contains("observed event")
                    || error.message.contains("triggering")
                    || error.message.contains("whereX")) =>
        {
            return uncovered("synthetic trigger context", path);
        }
        Err(error) => return (Outcome::Error(error.message), path),
    }
    let after = snapshot(&engine);
    if std::env::var("SMOKE_TRACE").is_ok() {
        eprintln!(
            "trace {} ({}): targets {targets:?}\n  before {before:?}\n  after {after:?}",
            card.name,
            path.name()
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
    // Targets that became illegal before resolution (a cost that sacrificed
    // or tapped the declared target) prove nothing about the effect.
    if engine.state.events[first_event..]
        .iter()
        .any(|event| matches!(event.kind.as_str(), "abilityFizzled" | "spellFizzled"))
    {
        return uncovered("fizzled: no legal target at resolution", path);
    }
    let mut observed = observe(&before, &after);
    observe_events(&mut observed, &engine.state.events[first_event..]);
    // The source's own fate (a sacrifice cost, state-based actions) is not
    // the effect's change, unless the source is itself a declared target.
    let source_targeted = targets.values().any(|target| {
        matches!(target, TargetRef::Permanent { instance_id } if instance_id == "smoke-source")
    }) || collections.iter().flatten().any(|(_, ids)| ids.contains("smoke-source"))
        || effects.iter().any(|effect| {
            ["recipient", "permanent", "object"]
                .iter()
                .any(|slot| value_kind(&effect[*slot]) == Some("self"))
        });
    if !source_targeted {
        observed.left.remove("smoke-source");
        observed.countered.remove("smoke-source");
        observed.tapped.remove("smoke-source");
    }
    if effects.iter().any(contingent) || rule.get("condition").is_some() {
        return judge_contingent(rule, &observed, path);
    }
    for (index, effect) in effects.iter().enumerate() {
        let Some(effect_kind) = value_kind(effect) else {
            continue;
        };
        let Some(expected) = promised(effect_kind) else {
            continue;
        };
        if !expected.iter().any(|kind| observed.kinds.contains(kind)) {
            if let Some(context) = unset_context(effect, &targets, permanent_source) {
                return uncovered(context, path);
            }
            if empty.contains(&index) {
                return uncovered("empty amount or collection in the fixture", path);
            }
            return (Outcome::NoChange(effect_kind.to_string()), path);
        }
    }
    // A single-collection rule changes exactly the objects its query names.
    if let [effect] = effects
        && let Some(Some((slot, expected))) = collections.first()
    {
        let reached = reached_collection(effect, &observed);
        if &reached != expected {
            let missing = expected.difference(&reached).cloned().collect::<Vec<_>>();
            let extra = reached.difference(expected).cloned().collect::<Vec<_>>();
            return (
                Outcome::WrongCount(format!(
                    "{} {slot}: missing {missing:?}, extra {extra:?}",
                    value_kind(effect).unwrap_or_default()
                )),
                path,
            );
        }
    }
    judge_targets(effects, &observed, &targets, path)
}

/// Every observation kind a contingent program could produce.
fn contingent_promises(rule: &Value) -> BTreeSet<&'static str> {
    let mut kinds = BTreeSet::new();
    let mut pending = vec![&rule["effects"]];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items.iter()),
            Value::Object(map) => {
                if let Some(kind) = map.get("kind").and_then(Value::as_str)
                    && let Some(promised) = promised(kind)
                {
                    kinds.extend(promised.iter().copied());
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }
    kinds
}

/// A contingent program (a choice, a condition, an earlier result) is
/// judged on the whole: something it can do happened, or nothing did and
/// the run proves nothing.
fn judge_contingent(rule: &Value, observed: &Observed, path: Path) -> (Outcome, Path) {
    let possible = contingent_promises(rule);
    if possible.is_empty() {
        return (
            Outcome::Uncovered("contingent program with no observable primitive".to_string()),
            path,
        );
    }
    // Only changes to objects other than the source's own fate count: a
    // permanent kind is kept when its set of reached objects is not empty.
    let reached = |kind: &str| match kind {
        "left" => !observed.left.is_empty(),
        "damage" => !observed.damaged.is_empty(),
        "counters" => !observed.countered.is_empty(),
        "tapped" => !observed.tapped.is_empty(),
        "stats" => !observed.stats.is_empty(),
        _ => observed.kinds.contains(kind),
    };
    if possible.iter().any(|kind| reached(kind)) {
        (Outcome::ContingentPass, path)
    } else {
        (
            Outcome::Uncovered("contingent program changed nothing".to_string()),
            path,
        )
    }
}

/// A combat trigger's resolution window: every promised primitive changed
/// something.
fn judge_window(
    effects: &[Value],
    observed: &Observed,
    rule: &Value,
    path: Path,
) -> (Outcome, Path) {
    if effects.iter().any(contingent) {
        return judge_contingent(rule, observed, path);
    }
    for effect in effects {
        let Some(effect_kind) = value_kind(effect) else {
            continue;
        };
        let Some(expected) = promised(effect_kind) else {
            continue;
        };
        if !expected.iter().any(|kind| observed.kinds.contains(kind)) {
            return (Outcome::NoChange(effect_kind.to_string()), path);
        }
    }
    (Outcome::Pass, path)
}

/// The declared targets of a single-primitive rule are each reached.
fn judge_targets(
    effects: &[Value],
    observed: &Observed,
    targets: &BTreeMap<String, TargetRef>,
    path: Path,
) -> (Outcome, Path) {
    if let [effect] = effects
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
        let declared = declared_permanent_targets(targets, id);
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
                path,
            );
        }
    }
    (Outcome::Pass, path)
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
            .split('|')
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
        .take(
            std::env::var("SMOKE_LIMIT")
                .ok()
                .and_then(|limit| limit.parse().ok())
                .unwrap_or(usize::MAX),
        )
        .collect::<Vec<_>>();
    let progress = std::env::var("SMOKE_PROGRESS").is_ok();
    let start = std::time::Instant::now();
    std::panic::set_hook(Box::new(|_| {}));
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&output).expect("smoke output"));
    let mut by_class = BTreeMap::<&'static str, usize>::new();
    let mut by_primitive = BTreeMap::<(String, &'static str), usize>::new();
    let mut card_class = BTreeMap::<&'static str, usize>::new();
    let mut by_path = BTreeMap::<(&'static str, &'static str), usize>::new();
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
        let card_start = std::time::Instant::now();
        let mut worst = Outcome::Pass;
        let mut any_exercised = false;
        let mut rules = Vec::new();
        for (rule_index, rule) in compilation.card.rules.iter().enumerate() {
            if progress {
                eprintln!("smoke: {} rule {rule_index}", row.name);
            }
            let (outcome, primitives, path) = smoke_rule(&compilation.card, rule, rule_index);
            *by_class.entry(outcome.class()).or_default() += 1;
            *by_path.entry((path.name(), outcome.class())).or_default() += 1;
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
                "path": path.name(),
            });
            if matches!(
                outcome,
                Outcome::Error(_)
                    | Outcome::NoTrigger(_)
                    | Outcome::NoChange(_)
                    | Outcome::WrongCount(_)
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
            json!({
                "oracle_id": row.oracle_id,
                "name": row.name,
                "class": class,
                "ms": card_start.elapsed().as_millis() as u64,
                "rules": rules,
            })
        )
        .expect("smoke output line");
        writer.flush().expect("smoke output flush");
    }
    let _ = std::panic::take_hook();
    eprintln!(
        "runtime_smoke: {} supported cards in {:.1}s",
        rows.len(),
        start.elapsed().as_secs_f64()
    );
    eprintln!("cards by class: {card_class:?}");
    eprintln!("rules by class: {by_class:?}");
    eprintln!("rules by path and class: {by_path:?}");
    let mut primitives = by_primitive.into_iter().collect::<Vec<_>>();
    primitives.sort_by(|a, b| b.1.cmp(&a.1));
    for ((primitive, class), count) in primitives.iter().take(60) {
        eprintln!("  {primitive:<32} {class:<10} {count}");
    }
}
