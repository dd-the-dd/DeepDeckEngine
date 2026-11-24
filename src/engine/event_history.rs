//! Event history: the one evaluator of `countEvents`, `countEventsThisTurn`
//! and `countEventsPreviousTurn` over the recorded game events. It replaces
//! the named history recipes (`scriedOrSurveilledThisTurn`,
//! `opponentSearchedLibraryThisTurn`, ...) with one typed pattern:
//!
//! ```text
//! { kind:     countEvents | countEventsThisTurn | countEventsPreviousTurn,
//!   event:    KIND | [KIND, ...]          recorded event kinds
//!   player?:  PlayerRelation              the event's player belongs to it
//!   relation?: controller | owner         which recorded player `player` reads
//!   where?:   card filter                 the event object's characteristics
//!   fromZone?: zone                       the recorded origin zone
//!   object?:  { kind: self }              the event object is the source
//!   excludeObject?: { kind: self }        the event object is not the source
//!   detail?:  { FIELD: scalar, ... }      exact recorded detail values; a
//!                                         FIELD starting with `/` is a JSON
//!                                         pointer into the detail
//!   relatedPlayers?: { FIELD: PlayerRelation }  detail fields naming players
//!   measure?: events | amount | count | cards
//!   aggregate?: total | greatestPerPlayer }
//! ```
//!
//! Soundness (no silent match): an unknown field value, an unevaluable
//! filter or an unresolvable player makes the count `None` (the condition is
//! false and the support check refuses the shape), never "every event". A
//! `player` relation is always resolved; it is never read as the controller.
//!
//! The `player` relation reads the event's player (the controller of a
//! departing permanent: Revolt's `a permanent you controlled left the
//! battlefield`), or, with `relation: "owner"`, a departing card's recorded
//! owner (`put into your graveyard`, CR 404.2). A relation counts the events
//! of players who have since lost the game.
//!
//! A recorded type line is read as words and phrases (`Time Lord`,
//! `Assembly-Worker`), and a departure's creature status is its last-known
//! one (`wasCreature`, CR 603.10a: an animated land that dies is a creature
//! that died).
//!
//! The previous turn of the first turn does not exist: its count is zero.
//!
//! `measure: cards` with a filter counts the cards of each arrival record
//! that satisfy it, read on the type lines the record captured (a token is
//! never a card).
use super::*;

/// Departures, which record the card's owner.
const OWNED_EVENTS: &[&str] = &["permanentDied", "permanentLeftBattlefield"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryScope {
    WholeGame,
    ThisTurn,
    PreviousTurn,
}

fn history_scope(expression: &Value) -> Option<HistoryScope> {
    match value_kind(expression) {
        Some("countEvents") => Some(HistoryScope::WholeGame),
        Some("countEventsThisTurn") => Some(HistoryScope::ThisTurn),
        Some("countEventsPreviousTurn") => Some(HistoryScope::PreviousTurn),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryMeasure {
    Events,
    Amount,
    Count,
    Cards,
}

fn history_measure(expression: &Value) -> Option<HistoryMeasure> {
    match expression.get("measure") {
        None => Some(HistoryMeasure::Events),
        Some(measure) => match measure.as_str()? {
            "events" => Some(HistoryMeasure::Events),
            "amount" => Some(HistoryMeasure::Amount),
            "count" => Some(HistoryMeasure::Count),
            "cards" => Some(HistoryMeasure::Cards),
            _ => None,
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryAggregate {
    Total,
    GreatestPerPlayer,
}

fn history_aggregate(expression: &Value) -> Option<HistoryAggregate> {
    match expression.get("aggregate") {
        None => Some(HistoryAggregate::Total),
        Some(aggregate) => match aggregate.as_str()? {
            "total" => Some(HistoryAggregate::Total),
            "greatestPerPlayer" => Some(HistoryAggregate::GreatestPerPlayer),
            _ => None,
        },
    }
}

fn event_kinds(expression: &Value) -> Option<Vec<&str>> {
    match &expression["event"] {
        Value::String(kind) => Some(vec![kind.as_str()]),
        Value::Array(kinds) if !kinds.is_empty() => kinds.iter().map(Value::as_str).collect(),
        _ => None,
    }
}

/// A player relation over event players, resolved for one evaluation.
#[derive(Clone, PartialEq, Eq)]
enum EventPlayers {
    /// Any player (the event must name one).
    Every,
    Is(String),
    /// Every player other than this one (`opponentsOf`, `playersExcept`).
    IsNot(String),
}

impl EventPlayers {
    fn contains(&self, player_id: Option<&str>) -> bool {
        let Some(player_id) = player_id else {
            return false;
        };
        match self {
            Self::Every => true,
            Self::Is(expected) => player_id == expected,
            Self::IsNot(excluded) => player_id != excluded,
        }
    }
}

fn player_relation_supported(relation: &Value) -> bool {
    match value_kind(relation) {
        Some("eachPlayer") => true,
        Some("opponentsOf" | "playersExcept") => {
            singular_player_reference_supported(&relation["player"])
        }
        _ => singular_player_reference_supported(relation),
    }
}

/// Which recorded player the `player` relation reads: the event's player, or
/// (`relation: "owner"`, departures only) the departing card's owner.
fn relation_supported(expression: &Value, kinds: &[&str]) -> bool {
    match expression.get("relation") {
        None => true,
        Some(relation) => match relation.as_str() {
            Some("controller") => true,
            Some("owner") => {
                expression.get("player").is_some()
                    && kinds.iter().all(|kind| OWNED_EVENTS.contains(kind))
            }
            _ => false,
        },
    }
}

fn resolve_player_relation(
    relation: &Value,
    resolve: &dyn Fn(&Value) -> Option<String>,
) -> Option<EventPlayers> {
    match value_kind(relation) {
        Some("eachPlayer") => Some(EventPlayers::Every),
        Some("opponentsOf" | "playersExcept") => {
            resolve(&relation["player"]).map(EventPlayers::IsNot)
        }
        _ => resolve(relation).map(EventPlayers::Is),
    }
}

fn scalar_detail_supported(detail: &Value) -> bool {
    detail.as_object().is_some_and(|fields| {
        fields
            .values()
            .all(|value| value.is_boolean() || value.is_string() || value.is_number())
    })
}

/// A card filter evaluable on an event object: printed characteristics only
/// (the object may have left the zone it was in).
fn event_filter_supported(filter: &Value) -> bool {
    filter.is_null()
        || (card_filter_supported(filter)
            && object_predicate::try_map_predicate_leaves(filter, 0, &mut |leaf| {
                if object_predicate::definition_atom_supported(leaf) {
                    Ok(leaf.clone())
                } else {
                    Err(EngineError::new("not a definition atom"))
                }
            })
            .is_ok())
}

/// The shapes the engine evaluates. Anything else is refused at the
/// executable-rule boundary instead of being counted loosely.
pub(super) fn event_history_supported(expression: &Value) -> bool {
    history_scope(expression).is_some()
        && event_kinds(expression).is_some_and(|kinds| relation_supported(expression, &kinds))
        && history_measure(expression).is_some()
        && history_aggregate(expression).is_some()
        && expression
            .get("player")
            .is_none_or(|relation| player_relation_supported(relation))
        && expression.get("where").is_none_or(event_filter_supported)
        && expression.get("fromZone").is_none_or(Value::is_string)
        && expression
            .get("object")
            .is_none_or(|object| matches!(value_kind(object), Some("self" | "abilitySource")))
        && expression
            .get("excludeObject")
            .is_none_or(|object| matches!(value_kind(object), Some("self" | "abilitySource")))
        && expression.get("detail").is_none_or(scalar_detail_supported)
        && expression.get("relatedPlayers").is_none_or(|related| {
            related.as_object().is_some_and(|fields| {
                fields
                    .values()
                    .all(|relation| player_relation_supported(relation))
            })
        })
}

/// The recorded detail value a pattern field names: a top-level field, or a
/// JSON pointer (`/decisions/useWarp`) into the recorded detail.
fn recorded_detail<'a>(detail: &'a Value, field: &str) -> Option<&'a Value> {
    if field.starts_with('/') {
        detail.pointer(field)
    } else {
        detail.get(field)
    }
}

/// The detail of one graveyard arrival: the card identities and, for each
/// card, the type line and token status it had as it arrived, so that a
/// later count reads them even after a token ceased to exist.
pub(super) fn graveyard_arrival_detail(cards: &[CardInstance]) -> Value {
    json!({
        "cardIds": cards
            .iter()
            .map(|card| card.instance_id.clone())
            .collect::<Vec<_>>(),
        "cards": cards
            .iter()
            .map(|card| json!({
                "id": card.instance_id,
                "typeLine": card.definition.type_line,
                "isToken": card.definition.is_token,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Lowercased words of a type line or a type phrase; a dash between types
/// and subtypes is its own word, a hyphen inside a subtype is kept.
fn phrase_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| word.trim_matches(|character: char| character == ',' || character == '.'))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The phrase as consecutive words of the type line (`Time Lord`, never the
/// `Assembly` of `Assembly-Worker`).
fn type_line_contains_phrase(type_line: &str, phrase: &str) -> bool {
    let line = phrase_words(type_line);
    let wanted = phrase_words(phrase);
    !wanted.is_empty()
        && line
            .windows(wanted.len())
            .any(|window| window == wanted.as_slice())
}

/// The type line an event recorded for its object (CR 603.10a last-known
/// type), evaluated for type atoms only; `was_creature` is the recorded
/// last-known creature status, when the event has one. `None` for any other
/// atom.
fn type_line_filter(
    type_line: &str,
    was_creature: Option<bool>,
    is_token: Option<bool>,
    filter: &Value,
) -> Option<bool> {
    match value_kind(filter) {
        None if filter.is_null() => Some(true),
        Some("cardTypeContains")
            if filter["value"]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case("Creature"))
                && was_creature.is_some() =>
        {
            was_creature
        }
        Some("cardTypeContains" | "typeLineContains") => Some(type_line_contains_phrase(
            type_line,
            filter["value"].as_str()?,
        )),
        Some("subtypeContains") => {
            let subtypes = split_type_line(type_line).map_or("", |(_, subtypes)| subtypes);
            Some(type_line_contains_phrase(
                subtypes,
                filter["value"].as_str()?,
            ))
        }
        Some("isLegendary") => Some(type_line_contains(type_line, "Legendary")),
        Some("isBasic") => Some(type_line_contains(type_line, "Basic")),
        Some("isToken") => is_token,
        // CR 110.4b: a card with a permanent card type; a token is no card.
        Some("isPermanentCard") => Some(
            !is_token?
                && [
                    "Artifact",
                    "Battle",
                    "Creature",
                    "Enchantment",
                    "Land",
                    "Planeswalker",
                ]
                .iter()
                .any(|card_type| type_line_contains_phrase(type_line, card_type)),
        ),
        Some("not") => type_line_filter(type_line, was_creature, is_token, &filter["operand"])
            .map(|value| !value),
        Some("and") => {
            let mut all = true;
            for operand in filter["operands"].as_array()? {
                all &= type_line_filter(type_line, was_creature, is_token, operand)?;
            }
            Some(all)
        }
        Some("or") => {
            let mut any = false;
            for operand in filter["operands"].as_array()? {
                any |= type_line_filter(type_line, was_creature, is_token, operand)?;
            }
            Some(any)
        }
        _ => None,
    }
}

impl GameEngine {
    fn event_object_definition(&self, instance_id: &str) -> Option<&CardDefinition> {
        if let Some((player_index, card_index)) = self.permanent_position(instance_id) {
            return Some(&self.state.players[player_index].battlefield[card_index].definition);
        }
        self.card_outside_battlefield(instance_id)
            .map(|card| &card.definition)
            .or_else(|| {
                self.state
                    .stack
                    .iter()
                    .find(|object| object.card.instance_id == instance_id)
                    .map(|object| &object.card.definition)
            })
    }

    /// The event object's filter: its recorded type line when the filter
    /// reads only type atoms, else the printed card it identifies. `None`
    /// when neither can answer.
    fn event_object_matches(&self, event: &GameEvent, filter: &Value) -> Option<bool> {
        if filter.is_null() {
            return Some(true);
        }
        if let Some(type_line) = event.detail["typeLine"].as_str()
            && let Some(matched) = type_line_filter(
                type_line,
                event.detail["wasCreature"].as_bool(),
                event.detail["wasToken"].as_bool(),
                filter,
            )
        {
            return Some(matched);
        }
        let definition = self.event_object_definition(event.card_instance_id.as_deref()?)?;
        Some(matches_card_filter(definition, filter))
    }

    /// The cards of one arrival record that satisfy `filter` (a token is
    /// never a card): the captured type line of each card, else its card.
    /// `None` when a card can be read neither way.
    fn arrival_cards_matching(&self, event: &GameEvent, filter: &Value) -> Option<i32> {
        let mut matched = 0_i32;
        for card_id in event.detail["cardIds"].as_array()? {
            let card_id = card_id.as_str()?;
            let captured = event.detail["cards"].as_array().and_then(|cards| {
                cards
                    .iter()
                    .find(|card| card["id"].as_str() == Some(card_id))
            });
            let matches = match captured {
                Some(card) if card["isToken"].as_bool()? => false,
                Some(card) => {
                    match type_line_filter(card["typeLine"].as_str()?, None, Some(false), filter) {
                        Some(matches) => matches,
                        None => matches_card_filter(self.event_object_definition(card_id)?, filter),
                    }
                }
                None => matches_card_filter(self.event_object_definition(card_id)?, filter),
            };
            if matches {
                matched = matched.saturating_add(1);
            }
        }
        Some(matched)
    }

    /// The per-event value of the measure; `None` when the event lacks the
    /// recorded quantity the measure reads.
    fn event_measure(event: &GameEvent, measure: HistoryMeasure) -> Option<i32> {
        match measure {
            HistoryMeasure::Events => Some(1),
            HistoryMeasure::Amount => {
                integer_value(&event.detail["amount"]).map(|value| value.max(0))
            }
            HistoryMeasure::Count => {
                integer_value(&event.detail["count"]).map(|value| value.max(0))
            }
            HistoryMeasure::Cards => event.detail["cardIds"]
                .as_array()
                .and_then(|cards| i32::try_from(cards.len()).ok()),
        }
    }

    /// Counts the recorded events of the pattern. `resolve` resolves one
    /// singular player reference of the evaluation context; `source` is the
    /// instance of the ability's source for `object: self`.
    fn count_event_history(
        &self,
        expression: &Value,
        source: Option<&str>,
        resolve: &dyn Fn(&Value) -> Option<String>,
    ) -> Option<i32> {
        let scope = history_scope(expression)?;
        let kinds = event_kinds(expression)?;
        let measure = history_measure(expression)?;
        let aggregate = history_aggregate(expression)?;
        let turn = match scope {
            HistoryScope::WholeGame => None,
            HistoryScope::ThisTurn => Some(self.state.turn_number),
            HistoryScope::PreviousTurn => match self.state.turn_number.checked_sub(1) {
                Some(previous) => Some(previous),
                None => return Some(0),
            },
        };
        let players = match expression.get("player") {
            None => None,
            Some(relation) => Some(resolve_player_relation(relation, resolve)?),
        };
        let owned = expression["relation"].as_str() == Some("owner");
        let related = match expression.get("relatedPlayers") {
            None => Vec::new(),
            Some(related) => related
                .as_object()?
                .iter()
                .map(|(field, relation)| {
                    resolve_player_relation(relation, resolve)
                        .map(|players| (field.as_str(), players))
                })
                .collect::<Option<Vec<_>>>()?,
        };
        let object = match expression.get("object") {
            None => None,
            Some(_) => Some(source?),
        };
        let excluded = match expression.get("excludeObject") {
            None => None,
            Some(_) => Some(source?),
        };
        let filter = expression.get("where").unwrap_or(&Value::Null);
        let detail = expression.get("detail").and_then(Value::as_object);
        let from_zone = expression.get("fromZone").and_then(Value::as_str);

        let mut total = 0_i32;
        let mut per_player = BTreeMap::<&str, i32>::new();
        for event in &self.state.events {
            if turn.is_some_and(|turn| event.turn_number != turn)
                || !kinds.contains(&event.kind.as_str())
                || players.as_ref().is_some_and(|players| {
                    !players.contains(if owned {
                        event.detail["owner"].as_str()
                    } else {
                        event.player_id.as_deref()
                    })
                })
                || object.is_some_and(|object| event.card_instance_id.as_deref() != Some(object))
                || excluded
                    .is_some_and(|excluded| event.card_instance_id.as_deref() == Some(excluded))
                || from_zone.is_some_and(|zone| event.detail["fromZone"].as_str() != Some(zone))
                || detail.is_some_and(|fields| {
                    fields.iter().any(|(field, expected)| {
                        recorded_detail(&event.detail, field) != Some(expected)
                    })
                })
                || related
                    .iter()
                    .any(|(field, players)| !players.contains(event.detail[*field].as_str()))
            {
                continue;
            }
            let value = if measure == HistoryMeasure::Cards && !filter.is_null() {
                self.arrival_cards_matching(event, filter)?
            } else {
                if !self.event_object_matches(event, filter)? {
                    continue;
                }
                Self::event_measure(event, measure)?
            };
            total = total.saturating_add(value);
            let tally = per_player
                .entry(event.player_id.as_deref().unwrap_or(""))
                .or_default();
            *tally = tally.saturating_add(value);
        }
        Some(match aggregate {
            HistoryAggregate::Total => total,
            HistoryAggregate::GreatestPerPlayer => per_player.values().copied().max().unwrap_or(0),
        })
    }

    /// Evaluation outside a resolving stack object (static abilities,
    /// intervening-if checks): players relative to the ability controller
    /// and its source.
    pub(super) fn event_history_count(
        &self,
        expression: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> Option<i32> {
        self.count_event_history(
            expression,
            source.map(|source| source.instance_id.as_str()),
            &|reference| {
                numeric_evaluation::source_relative_numeric_player(reference, controller_id, source)
                    .map(str::to_string)
            },
        )
    }

    /// Evaluation during resolution: players resolved with the stack
    /// object's bindings.
    pub(super) fn runtime_event_history_count(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<i32> {
        self.count_event_history(
            expression,
            Some(stack_object.card.instance_id.as_str()),
            &|reference| self.resolve_player(reference, stack_object, bindings),
        )
    }
}

/// The players a damage history relation names (`to you`, `to an opponent`,
/// `to a player`) relative to the ability controller. An unknown relation
/// names no player, so the predicate is false and refused by its support
/// check rather than matching every damage event.
fn damaged_player_relation_holds(
    relation: &str,
    damaged: &str,
    ability_controller: &str,
) -> Option<bool> {
    match relation {
        "abilityController" => Some(damaged == ability_controller),
        // Two-player and free-for-all games: every other player is an
        // opponent (CR 102.2, 102.3).
        "opponentOfAbilityController" => Some(damaged != ability_controller),
        "anyPlayer" => Some(true),
        _ => None,
    }
}

/// Whether `{ kind: dealtDamageToPlayerThisTurn, recipient, combatOnly }`
/// is well formed.
pub(super) fn dealt_damage_to_player_supported(filter: &Value) -> bool {
    filter["recipient"]
        .as_str()
        .is_some_and(|relation| damaged_player_relation_holds(relation, "", "").is_some())
        && filter.get("combatOnly").is_none_or(Value::is_boolean)
}

impl GameEngine {
    /// `that dealt [combat] damage to PLAYER this turn` (CR 120.1, 510.2):
    /// a damage event of this turn whose source is this permanent, whose
    /// recipient is a player of the relation (relative to the ability
    /// controller) and, when `combatOnly`, that was combat damage. Only
    /// positive amounts are damage dealt (CR 120.8).
    pub(super) fn permanent_dealt_damage_to_player_this_turn(
        &self,
        permanent: &CardInstance,
        filter: &Value,
        ability_controller: &str,
    ) -> bool {
        let Some(relation) = filter["recipient"].as_str() else {
            return false;
        };
        let combat_only = filter["combatOnly"].as_bool().unwrap_or(false);
        self.state.events.iter().any(|event| {
            event.turn_number == self.state.turn_number
                && event.kind == "damageDealt"
                && event.card_instance_id.is_none()
                && event.detail["source"].as_str() == Some(permanent.instance_id.as_str())
                && event.detail["amount"]
                    .as_i64()
                    .is_some_and(|amount| amount > 0)
                && (!combat_only || event.detail["combat"].as_bool() == Some(true))
                && event.player_id.as_deref().is_some_and(|damaged| {
                    damaged_player_relation_holds(relation, damaged, ability_controller)
                        .unwrap_or(false)
                })
        })
    }
}
