//! Keyword definitions compile to ordinary reusable runtime instructions.

use serde_json::{Value, json};

use std::collections::{BTreeMap, BTreeSet};

use super::{
    CardDefinition, CardInstance, CastingDeclaration, DecisionProvider, EngineError, GameEngine,
    LegalAction, MAX_DECLARATION_OPTIONS, card_filter_supported, card_matches_protection_quality,
    cost_supported, distinct_card_types, integer_value, kicker_ability_supported, mana_value,
    matches_card_filter, player_reference_supported, value_kind,
};

/// The declared choice of a kicker cost that is not mana (CR 702.33a).
const KICKER_COST_TARGET: &str = "kickerCost";

/// The one nonmana kicker cost of a card (`Kicker—Sacrifice a land.`).
/// Mana kicker costs are added to the spell's total cost instead; a
/// multi-kicker (`costs`) lists mana alternatives.
fn nonmana_kicker_cost(card: &CardDefinition) -> Option<&Value> {
    card.printed_rules().iter().find_map(|rule| {
        let ability = &rule["ability"];
        (value_kind(rule) == Some("keywordAbility")
            && kicker_ability_supported(ability)
            && !ability["costs"].is_array()
            && value_kind(&ability["cost"]) != Some("payMana"))
        .then_some(&ability["cost"])
    })
}

fn kicked(decisions: &BTreeMap<String, Value>) -> bool {
    decisions.get("kicked").and_then(Value::as_bool) == Some(true)
}

impl GameEngine {
    /// The declarations of a kicked cast: a nonmana kicker cost declares the
    /// object it pays with, like any keyword cost; without a payable object
    /// the spell cannot be kicked.
    pub(super) fn kicker_cost_declarations(
        &self,
        kicked_declaration: CastingDeclaration,
        ability: &Value,
        controller_id: &str,
    ) -> Vec<CastingDeclaration> {
        let cost = &ability["cost"];
        if ability["costs"].is_array() || value_kind(cost) == Some("payMana") {
            return vec![kicked_declaration];
        }
        self.declarations_for_keyword_cost(
            vec![kicked_declaration],
            cost,
            controller_id,
            KICKER_COST_TARGET,
        )
        .unwrap_or_default()
    }

    /// The declared kicker payment is a cost choice, never a spell target.
    pub(super) fn kicker_cost_target_ids(
        card: &CardDefinition,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        nonmana_kicker_cost(card)
            .filter(|cost| {
                kicked(decisions)
                    && matches!(value_kind(cost), Some("sacrificePermanent" | "discardCard"))
            })
            .map(|_| vec![KICKER_COST_TARGET.to_string()])
            .unwrap_or_default()
    }

    /// Pays a nonmana kicker cost of a kicked spell as it is cast (CR
    /// 601.2h), with its declared object.
    pub(super) fn apply_kicker_keyword_cost<P: DecisionProvider>(
        &mut self,
        action: &LegalAction,
        card: &CardDefinition,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        if !kicked(&action.decisions) {
            return Ok(());
        }
        let Some(cost) = nonmana_kicker_cost(card).cloned() else {
            return Ok(());
        };
        self.apply_keyword_cost(action, &cost, KICKER_COST_TARGET, provider)
    }
}

/// CR 702.11d: `hexproof from [quality]`, each quality one the protection
/// evaluator reads (a color, a card type, `everything`).
pub(super) fn hexproof_from_supported(value: &Value) -> bool {
    value_kind(value) == Some("hexproofFrom")
        && value["from"].as_array().is_some_and(|qualities| {
            !qualities.is_empty() && qualities.iter().all(Value::is_string)
        })
}

/// Whether a printed `hexproof from` of `permanent` covers spells and
/// abilities from `source` (CR 702.11d). Hexproof only stops opponents: the
/// caller checks the targeting player.
pub(super) fn printed_hexproof_from(permanent: &CardInstance, source: &CardDefinition) -> bool {
    permanent.definition.printed_rules().iter().any(|rule| {
        let abilities: Vec<&Value> = match value_kind(rule) {
            Some("keywordAbility") => vec![&rule["ability"]],
            Some("keywordAbilityGroup") => rule["abilities"]
                .as_array()
                .map(|abilities| abilities.iter().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        abilities.into_iter().any(|ability| {
            hexproof_from_supported(ability)
                && ability["from"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|quality| card_matches_protection_quality(source, quality))
        })
    })
}

/// The objects an alternative cost's object payment chose, per cost index
/// (CR 601.2h): permanents to sacrifice or tap, cards to discard.
fn alternative_objects_key(index: usize) -> String {
    format!("alternativeObjects:{index}")
}

/// The generic mana the sacrificed permanents' mana values take off the
/// spell's total cost (emerge, CR 702.119a).
const ALTERNATIVE_REDUCTION: &str = "alternativeCostGenericReduction";

/// An object payment of an alternative cost: `count` chosen permanents the
/// caster controls to sacrifice or tap, or cards in hand to discard, each
/// matching `where`. A sacrifice with `reduceByManaValue` reduces the total
/// cost by the sacrificed mana values (emerge).
pub(super) fn alternative_object_cost_supported(cost: &Value) -> bool {
    matches!(
        value_kind(cost),
        Some("sacrificePermanent" | "tap" | "discardCard")
    ) && card_filter_supported(&cost["where"])
        && cost
            .get("count")
            .is_none_or(|count| integer_value(count).is_some_and(|count| count > 0))
        && cost.get("reduceByManaValue").is_none_or(Value::is_boolean)
}

fn alternative_object_costs(ability: &Value) -> Vec<(usize, Value)> {
    ability["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, cost)| alternative_object_cost_supported(cost))
        .map(|(index, cost)| (index, cost.clone()))
        .collect()
}

fn alternative_object_count(cost: &Value) -> usize {
    integer_value(&cost["count"])
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or(1)
}

fn selected_alternative_ability(decisions: &BTreeMap<String, Value>) -> Option<&Value> {
    decisions
        .get("effectiveAlternativeCost")
        .filter(|_| decisions.get("useAlternativeCost").and_then(Value::as_bool) == Some(true))
}

/// The generic reduction a declared alternative sacrifice grants.
pub(super) fn alternative_cost_generic_reduction(decisions: &BTreeMap<String, Value>) -> i32 {
    decisions
        .get(ALTERNATIVE_REDUCTION)
        .and_then(integer_value)
        .unwrap_or(0)
        .max(0)
}

impl GameEngine {
    fn alternative_object_candidates(
        &self,
        cost: &Value,
        controller_id: &str,
        casting_card: Option<&str>,
    ) -> Vec<String> {
        if value_kind(cost) == Some("discardCard") {
            return self
                .player_index(controller_id)
                .ok()
                .map(|player_index| {
                    self.state.players[player_index]
                        .hand
                        .iter()
                        .filter(|card| Some(card.instance_id.as_str()) != casting_card)
                        .filter(|card| matches_card_filter(&card.definition, &cost["where"]))
                        .map(|card| card.instance_id.clone())
                        .collect()
                })
                .unwrap_or_default();
        }
        let tap = value_kind(cost) == Some("tap");
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| {
                permanent.controller == controller_id
                    && !(tap && permanent.tapped)
                    && self.permanent_matches_effective_filter(permanent, &cost["where"])
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect()
    }

    fn permanent_mana_value(&self, instance_id: &str) -> i32 {
        self.permanent_position(instance_id)
            .map(|(player_index, card_index)| {
                mana_value(&self.state.players[player_index].battlefield[card_index].definition)
            })
            .unwrap_or(0)
    }

    /// Each declaration using an alternative cost with object payments
    /// declares distinct objects for each of them; without enough payable
    /// objects the alternative is unavailable.
    pub(super) fn expand_alternative_object_declarations(
        &self,
        controller_id: &str,
        casting_source: Option<&CardInstance>,
        assignments: Vec<CastingDeclaration>,
    ) -> Vec<CastingDeclaration> {
        let casting_card = casting_source.map(|card| card.instance_id.as_str());
        let mut expanded = Vec::new();
        for assignment in assignments {
            let costs = selected_alternative_ability(&assignment.decisions)
                .map(alternative_object_costs)
                .unwrap_or_default();
            if costs.is_empty() {
                expanded.push(assignment);
                continue;
            }
            let mut partial = vec![assignment];
            for (index, cost) in &costs {
                let candidates =
                    self.alternative_object_candidates(cost, controller_id, casting_card);
                let count = alternative_object_count(cost);
                let reduces = cost["reduceByManaValue"].as_bool() == Some(true);
                let mut next = Vec::new();
                for declaration in partial {
                    let used = costs
                        .iter()
                        .filter_map(|(other, _)| {
                            declaration.decisions.get(&alternative_objects_key(*other))
                        })
                        .flat_map(|ids| ids.as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<BTreeSet<_>>();
                    let available = candidates
                        .iter()
                        .filter(|id| !used.contains(*id))
                        .cloned()
                        .collect::<Vec<_>>();
                    for selection in
                        Self::resolution_selections(&available, count, MAX_DECLARATION_OPTIONS)
                    {
                        let mut declared = declaration.clone();
                        if reduces {
                            let reduction = selection
                                .iter()
                                .map(|id| self.permanent_mana_value(id))
                                .sum::<i32>()
                                .saturating_add(alternative_cost_generic_reduction(
                                    &declared.decisions,
                                ));
                            declared
                                .decisions
                                .insert(ALTERNATIVE_REDUCTION.to_string(), Value::from(reduction));
                        }
                        declared.decisions.insert(
                            alternative_objects_key(*index),
                            Value::Array(selection.into_iter().map(Value::String).collect()),
                        );
                        next.push(declared);
                        if next.len() >= MAX_DECLARATION_OPTIONS {
                            break;
                        }
                    }
                }
                partial = next;
            }
            expanded.extend(partial);
            if expanded.len() >= MAX_DECLARATION_OPTIONS {
                break;
            }
        }
        expanded.truncate(MAX_DECLARATION_OPTIONS);
        expanded
    }

    /// Pays the declared object payments of a selected alternative cost:
    /// sacrifices, taps and discards each chosen object (CR 601.2h).
    pub(super) fn apply_alternative_object_payments(
        &mut self,
        action: &LegalAction,
        ability: &Value,
    ) -> Result<Option<String>, EngineError> {
        let mut paid_object = None;
        let player_index = self.player_index(&action.player_id)?;
        for (index, cost) in alternative_object_costs(ability) {
            let ids = action
                .decisions
                .get(&alternative_objects_key(index))
                .and_then(Value::as_array)
                .ok_or_else(|| EngineError::new("an alternative-cost object payment is missing"))?
                .iter()
                .map(|id| {
                    id.as_str()
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| EngineError::new("an alternative-cost object id is invalid"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if ids.len() != alternative_object_count(&cost)
                || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            {
                return Err(EngineError::new(
                    "the alternative-cost object payment chose the wrong objects",
                ));
            }
            for id in ids {
                match value_kind(&cost) {
                    Some("discardCard") => {
                        let card_index = self.state.players[player_index]
                            .hand
                            .iter()
                            .position(|card| {
                                card.instance_id == id
                                    && matches_card_filter(&card.definition, &cost["where"])
                            })
                            .ok_or_else(|| {
                                EngineError::new(
                                    "the alternative-cost discard is no longer payable",
                                )
                            })?;
                        let (discarded, destination) =
                            self.take_zone_card_for_graveyard(player_index, "hand", card_index)?;
                        self.put_owned_card_into_prepared_graveyard(
                            player_index,
                            discarded,
                            destination,
                        )?;
                        self.record_event(
                            "cardDiscarded",
                            Some(action.player_id.clone()),
                            Some(id.clone()),
                            json!({ "reason": "cost" }),
                        );
                        self.enqueue_card_discarded_triggers(&action.player_id);
                    }
                    kind => {
                        let Some((owner, card_index)) = self.permanent_position(&id) else {
                            return Err(EngineError::new(
                                "an alternative-cost permanent left the battlefield",
                            ));
                        };
                        let permanent = &self.state.players[owner].battlefield[card_index];
                        if permanent.controller != action.player_id
                            || !self.permanent_matches_effective_filter(permanent, &cost["where"])
                            || (kind == Some("tap") && permanent.tapped)
                        {
                            return Err(EngineError::new(
                                "the permanent cannot pay the alternative cost",
                            ));
                        }
                        if kind == Some("tap") {
                            let tapped = self.tap_permanent(&id)?.ok_or_else(|| {
                                EngineError::new("the alternative-cost tap could not be paid")
                            })?;
                            self.enqueue_permanent_tapped_triggers(&tapped);
                        } else {
                            self.move_permanent_to_graveyard(&id, "sacrificed")?;
                        }
                    }
                }
                paid_object = Some(id);
            }
        }
        Ok(paid_object)
    }
}

/// The ability that lets a card be cast from its owner's graveyard by
/// discarding a card in addition to its other costs: retrace (CR 702.81a,
/// a land card) and jump-start (CR 702.133a, any card, then exiled when it
/// leaves the stack).
fn graveyard_discard_cast(card: &CardDefinition) -> Option<&Value> {
    card.printed_rules().iter().find_map(|rule| {
        (value_kind(rule) == Some("keywordAbility")
            && value_kind(&rule["ability"]) == Some("graveyardCastWithDiscard"))
        .then_some(&rule["ability"])
    })
}

/// Whether `candidate` may be discarded to cast `card` from its graveyard.
pub(super) fn graveyard_discard_candidate(
    card: &CardDefinition,
    candidate: &CardDefinition,
) -> bool {
    graveyard_discard_cast(card).is_some_and(|ability| {
        ability
            .get("where")
            .is_none_or(|filter| filter.is_null() || matches_card_filter(candidate, filter))
    })
}

/// CR 702.133a: a spell cast with jump-start is exiled instead of going
/// anywhere else when it leaves the stack.
pub(super) fn mark_graveyard_discard_cast_exile(
    card: &CardDefinition,
    declarations: &mut [CastingDeclaration],
) {
    if !graveyard_discard_cast(card)
        .is_some_and(|ability| ability["exileOnLeaveStack"].as_bool() == Some(true))
    {
        return;
    }
    for declaration in declarations {
        declaration
            .decisions
            .insert("exileOnLeaveStack".to_string(), Value::Bool(true));
    }
}

/// CR 702.42a: `Entwine [mana cost]`.
pub(super) fn entwine_ability_supported(value: &Value) -> bool {
    value_kind(value) == Some("entwine")
        && value_kind(&value["cost"]) == Some("payMana")
        && cost_supported(&value["cost"])
}

fn entwine_mana_cost(card: &CardDefinition) -> Option<&str> {
    card.printed_rules().iter().find_map(|rule| {
        (value_kind(rule) == Some("keywordAbility") && entwine_ability_supported(&rule["ability"]))
            .then(|| rule["ability"]["cost"]["manaCost"].as_str())
            .flatten()
    })
}

const ENTWINED: &str = "entwined";

/// CR 702.42a: a modal spell with entwine may choose all of its modes
/// instead of the number its header allows; that declaration is marked
/// `entwined` and pays the entwine cost. A header that already allows every
/// mode adds nothing.
pub(super) fn entwined_mode_declarations(
    card: &CardDefinition,
    decision: &Value,
    id: &str,
    assignment: &CastingDeclaration,
    selections: &[Value],
) -> Vec<CastingDeclaration> {
    let Some(options) = decision["options"].as_array() else {
        return Vec::new();
    };
    let every_mode = Value::Array(options.clone());
    if entwine_mana_cost(card).is_none()
        || options.len() < 2
        || selections.contains(&every_mode)
        || assignment.decisions.contains_key(ENTWINED)
    {
        return Vec::new();
    }
    let mut entwined = assignment.clone();
    entwined.decisions.insert(id.to_string(), every_mode);
    entwined
        .decisions
        .insert(ENTWINED.to_string(), Value::Bool(true));
    vec![entwined]
}

/// The entwine cost an entwined declaration adds to the spell (CR 601.2f).
pub(super) fn entwine_mana_costs(
    card: &CardDefinition,
    decisions: &BTreeMap<String, Value>,
) -> Vec<String> {
    if decisions.get(ENTWINED).and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    entwine_mana_cost(card)
        .map(|cost| vec![cost.to_string()])
        .unwrap_or_default()
}

/// Morph (CR 702.37a) and disguise (CR 702.168a) both cast their card face
/// down as a 2/2 creature for {3} and turn it face up for the keyword cost.
pub(super) fn is_face_down_casting_keyword(value: &Value) -> bool {
    matches!(value_kind(value), Some("morph" | "disguise"))
}

/// The rules of the face-down 2/2 a card is cast as (CR 708.2a): none for
/// morph, ward {2} for disguise (CR 702.168a).
pub(super) fn face_down_rules(card: &CardDefinition) -> Vec<Value> {
    let disguised = card.printed_rules().iter().any(|rule| {
        value_kind(rule) == Some("keywordAbility")
            && value_kind(&rule["ability"]) == Some("disguise")
    });
    if !disguised {
        return Vec::new();
    }
    vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "ward", "cost": { "kind": "payMana", "manaCost": "{2}" } },
    })]
}

/// The graveyard cards an escape cost exiles (CR 702.138a): exactly N other
/// cards (`quantity: exactly`), or any number of them satisfying an
/// aggregate condition over the chosen set (`distinctCardTypesAtLeast`).
fn escape_exile_quantity(exile_cost: &Value) -> Option<EscapeExileQuantity> {
    match value_kind(&exile_cost["quantity"]) {
        Some("exactly") => integer_value(&exile_cost["quantity"]["count"])
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| *count > 0)
            .map(EscapeExileQuantity::Exactly),
        Some("anyNumber")
            if value_kind(&exile_cost["aggregateCondition"])
                == Some("distinctCardTypesAtLeast") =>
        {
            integer_value(&exile_cost["aggregateCondition"]["count"])
                .filter(|count| *count > 0)
                .map(EscapeExileQuantity::DistinctCardTypesAtLeast)
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum EscapeExileQuantity {
    Exactly(usize),
    DistinctCardTypesAtLeast(i32),
}

/// The escape keyword shape the runtime pays: a mana payment and the
/// graveyard exile of other cards.
pub(super) fn escape_ability_supported(value: &Value) -> bool {
    value_kind(value) == Some("escape")
        && value["costs"].as_array().is_some_and(|costs| {
            costs.len() == 2
                && value_kind(&costs[0]) == Some("payMana")
                && cost_supported(&costs[0])
                && value_kind(&costs[1]) == Some("exileGraveyardCards")
                && player_reference_supported(&costs[1]["player"])
                && card_filter_supported(&costs[1]["where"])
                && costs[1]["excludeSource"].is_boolean()
                && escape_exile_quantity(&costs[1]).is_some()
                && costs[1]["decisionId"].is_string()
        })
}

/// The sizes of the graveyard selections that can pay the escape exile
/// among `candidates` cards.
pub(super) fn escape_selection_sizes(
    exile_cost: &Value,
    candidates: usize,
) -> std::ops::RangeInclusive<usize> {
    match escape_exile_quantity(exile_cost) {
        Some(EscapeExileQuantity::Exactly(count)) => count..=count,
        Some(EscapeExileQuantity::DistinctCardTypesAtLeast(_)) => 1..=candidates,
        // An unsupported shape admits no selection (the range is empty).
        None => 1..=0,
    }
}

/// Whether the chosen graveyard cards pay the escape exile.
pub(super) fn escape_selection_satisfies<'a>(
    exile_cost: &Value,
    selected: impl IntoIterator<Item = &'a CardDefinition>,
) -> bool {
    let selected = selected.into_iter().collect::<Vec<_>>();
    match escape_exile_quantity(exile_cost) {
        Some(EscapeExileQuantity::Exactly(count)) => selected.len() == count,
        Some(EscapeExileQuantity::DistinctCardTypesAtLeast(minimum)) => {
            !selected.is_empty()
                && distinct_card_types(selected.into_iter()).len() as i32 >= minimum
        }
        None => false,
    }
}

/// The source mana value is captured from the resulting spell at cast time,
/// including its chosen X. Each trigger owns its rule and transient bindings;
/// resolving or countering the source cannot change the captured threshold.
pub(super) fn cascade_trigger_rule(source_mana_value: i32) -> Value {
    let controller = json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    let below_source = json!({
        "kind": "compare",
        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
        "operator": "<",
        "right": { "kind": "integer", "value": source_mana_value },
    });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "exileFromTopUntil",
                "zone": { "kind": "library", "player": controller.clone() },
                "bind": "cascadeExiled",
                "stopCardBind": "cascadeHit",
                "faceDown": false,
                "stopWhere": {
                    "kind": "and",
                    "operands": [
                        { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } },
                        below_source.clone(),
                    ],
                },
                "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
            },
            {
                "kind": "castOneCard",
                "player": controller.clone(),
                "cards": { "kind": "boundObjects", "binding": "cascadeHit" },
                "sourceZone": "exile",
                "normalCostsApply": false,
                "optional": true,
                "spellWhere": below_source,
                "bind": "cascadeCast",
            },
            {
                "kind": "moveCards",
                "cards": { "kind": "boundObjects", "binding": "cascadeExiled" },
                "from": { "kind": "exile", "player": controller.clone() },
                "to": { "kind": "library", "player": controller, "position": "bottom" },
                "order": { "kind": "random" },
            },
        ],
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "keyword_programs_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "casting_keyword_runtime_tests.rs"]
mod casting_keyword_runtime_tests;
