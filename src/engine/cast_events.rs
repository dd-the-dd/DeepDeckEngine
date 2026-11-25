//! Generic event model, cast family (docs/oracle-event-model.md).
//!
//! Casting a spell is one record: the caster, the spell's definition, its
//! cast decisions (source zone, X, modes), its targets and the caster's
//! per-turn spell history. A `spellCast` trigger event is a pattern of the
//! same shape: the subject relation (any spell, or the source spell itself
//! observed from the stack, CR 601.2i), the caster relation, a spell
//! predicate, and modifiers (per-turn ordinal over matching spells, origin
//! zone, turn relations, stored-choice relations). One matcher evaluates it.
use super::*;

/// The flag a cast spell keeps when it was cast during its caster's own main
/// phase (the Addendum timing, `If you cast this spell during your main
/// phase`).
pub(super) const CAST_DURING_MAIN_PHASE_FLAG: &str = "wasCastDuringMainPhase";

/// Every flag a spell records only because it was cast. A copy of a spell
/// copies the original's copiable values and choices (CR 707.10) but is not
/// cast, so none of these cast-history facts carry over to it.
pub(super) const CAST_HISTORY_FLAGS: [&str; 3] =
    ["wasCast", "wasCastFromHand", CAST_DURING_MAIN_PHASE_FLAG];

/// Marks `flags` as those of a spell copy: the copy marker is set and every
/// cast-history flag inherited from the original is removed (CR 707.10).
pub(super) fn mark_spell_copy(flags: &mut BTreeMap<String, bool>) {
    flags.insert("isSpellCopy".to_string(), true);
    for flag in CAST_HISTORY_FLAGS {
        flags.remove(flag);
    }
}

impl GameEngine {
    /// True while `player_id` is the active player in a main phase: a spell
    /// that player casts now is cast during their main phase.
    pub(super) fn casting_during_own_main_phase(&self, player_id: &str) -> bool {
        self.state.players[self.state.active_player].id == player_id
            && matches!(
                self.state.step,
                GameStep::PrecombatMain | GameStep::PostcombatMain
            )
    }
}

/// One spell cast.
pub(super) struct CastEventRecord<'a> {
    pub caster: &'a str,
    /// The spell's owner (whose zone it was cast from, CR 400.3).
    pub owner: Option<&'a str>,
    pub spell: &'a CardDefinition,
    pub decisions: &'a BTreeMap<String, Value>,
    pub targets: &'a BTreeMap<String, TargetRef>,
}

/// How the pattern's spell relates to the observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CastSubject {
    /// Any spell matching the predicate, observed by a permanent or card.
    AnySpell,
    /// The observer is the cast spell (`When you cast this spell`).
    SourceSpell,
}

pub(super) fn cast_subject(event: &Value) -> CastSubject {
    if event["sourceIsSelf"].as_bool() == Some(true) {
        CastSubject::SourceSpell
    } else {
        CastSubject::AnySpell
    }
}

fn cast_target_alternative_supported(target: &Value) -> bool {
    match value_kind(target) {
        Some("self") => true,
        Some("player") => player_action_events::event_player_reference_supported(&target["player"]),
        Some("permanents") => {
            target.get("where").is_none_or(card_filter_supported)
                && target
                    .get("controller")
                    .is_none_or(player_action_events::event_player_reference_supported)
        }
        _ => false,
    }
}

/// Modifiers of a `spellCast` pattern beyond its historical fields.
pub(super) fn spell_cast_modifiers_supported(event: &Value) -> bool {
    event.get("fromOwnZone").is_none_or(Value::is_boolean)
        && event.get("fromZone").is_none_or(Value::is_string)
        && event.get("fromZoneNot").is_none_or(Value::is_string)
        && event
            .get("duringOpponentTurn")
            .is_none_or(Value::is_boolean)
        && event
            .get("duringControllerTurn")
            .is_none_or(Value::is_boolean)
        && event.get("targets").is_none_or(|targets| {
            targets["only"].is_boolean()
                && targets["anyOf"].as_array().is_some_and(|alternatives| {
                    !alternatives.is_empty()
                        && alternatives.iter().all(cast_target_alternative_supported)
                })
        })
}

impl GameEngine {
    /// Whether one target of the cast spell is one of the alternatives.
    fn cast_target_matches(
        &self,
        alternatives: &[Value],
        target: &TargetRef,
        observer: &CardInstance,
    ) -> bool {
        alternatives
            .iter()
            .any(|alternative| match (value_kind(alternative), target) {
                (Some("self"), TargetRef::Permanent { instance_id }) => {
                    *instance_id == observer.instance_id
                }
                (Some("player"), TargetRef::Player { player_id }) => {
                    self.event_player_reference_matches(&alternative["player"], observer, player_id)
                }
                (Some("permanents"), TargetRef::Permanent { instance_id }) => self
                    .permanent_position(instance_id)
                    .is_some_and(|(player_index, card_index)| {
                        let permanent = &self.state.players[player_index].battlefield[card_index];
                        alternative.get("controller").is_none_or(|controller| {
                            self.event_player_reference_matches(
                                controller,
                                observer,
                                &permanent.controller,
                            )
                        }) && alternative.get("where").is_none_or(|filter| {
                            self.permanent_matches_effective_filter(permanent, filter)
                        })
                    }),
                _ => false,
            })
    }

    /// `that targets [only] …` on the cast record's targets (CR 115.1).
    fn cast_targets_match(
        &self,
        pattern: &Value,
        record: &CastEventRecord<'_>,
        observer: &CardInstance,
    ) -> bool {
        let Some(alternatives) = pattern["anyOf"].as_array() else {
            return false;
        };
        let mut targets = record.targets.values();
        if pattern["only"].as_bool() == Some(true) {
            !record.targets.is_empty()
                && targets.all(|target| self.cast_target_matches(alternatives, target, observer))
        } else {
            targets.any(|target| self.cast_target_matches(alternatives, target, observer))
        }
    }

    /// A numeric characteristic of the spell on the stack, or `None` for a
    /// property the cast record does not hold.
    fn spell_numeric_property(record: &CastEventRecord<'_>, property: &Value) -> Option<i32> {
        if value_kind(&property["object"]) != Some("candidate") {
            return None;
        }
        match value_kind(property)? {
            // CR 202.3e: X in the mana cost is the announced value.
            "manaValueOf" => Some(mana_value_with_decisions(record.spell, record.decisions)),
            "powerOf" => record.spell.power.as_deref()?.trim().parse().ok(),
            "toughnessOf" => record.spell.toughness.as_deref()?.trim().parse().ok(),
            _ => None,
        }
    }

    /// The spell predicate on the cast record: numeric comparisons read the
    /// stack characteristics, every other atom the spell's definition.
    pub(super) fn cast_spell_matches(&self, filter: &Value, record: &CastEventRecord<'_>) -> bool {
        match value_kind(filter) {
            None if filter.is_null() => true,
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .all(|operand| self.cast_spell_matches(operand, record))
            }),
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .any(|operand| self.cast_spell_matches(operand, record))
            }),
            Some("not") => !self.cast_spell_matches(&filter["operand"], record),
            Some("compare") if value_kind(&filter["left"]["object"]) == Some("candidate") => {
                let (Some(left), Some(right)) = (
                    Self::spell_numeric_property(record, &filter["left"]),
                    integer_value(&filter["right"]),
                ) else {
                    // A property the stack record does not hold (color
                    // count, …) keeps the definition evaluator.
                    return matches_card_filter(record.spell, filter);
                };
                let right = right as i32;
                match filter["operator"].as_str() {
                    Some("==") => left == right,
                    Some("<=") => left <= right,
                    Some(">=") => left >= right,
                    Some("<") => left < right,
                    Some(">") => left > right,
                    _ => false,
                }
            }
            _ => matches_card_filter(record.spell, filter),
        }
    }

    /// The caster's per-turn ordinal of this spell among the spells that
    /// satisfy `filter` (the spell itself is already logged).
    fn cast_turn_ordinal(&self, caster: &str, filter: &Value) -> usize {
        self.state
            .events
            .iter()
            .filter(|recorded| {
                recorded.turn_number == self.state.turn_number
                    && recorded.kind == "spellCast"
                    && recorded.player_id.as_deref() == Some(caster)
                    && (filter.is_null() || spell_cast_event_matches_filter(recorded, filter))
            })
            .count()
    }

    /// The one matcher of the cast family for a `spellCast` pattern.
    pub(super) fn spell_cast_matches(
        &self,
        event: &Value,
        record: &CastEventRecord<'_>,
        source: &CardInstance,
        subject: CastSubject,
    ) -> bool {
        if value_kind(event) != Some("spellCast") || cast_subject(event) != subject {
            return false;
        }
        let spell = record.spell;
        let casting_player_id = record.caster;
        let cast_decisions = record.decisions;
        let cast_source_zone = cast_decisions
            .get("castSourceZone")
            .and_then(Value::as_str)
            .unwrap_or("hand");
        let from_zone_matches = event["fromZone"]
            .as_str()
            .is_none_or(|from_zone| cast_source_zone == from_zone)
            && event["fromZoneNot"]
                .as_str()
                .is_none_or(|from_zone| cast_source_zone != from_zone);
        // The turn relation is read against the ability controller (CR 500.1):
        // `during your turn` / `during an opponent's turn`.
        let active_player_id = &self.state.players[self.state.active_player].id;
        let turn_matches = event["duringOpponentTurn"].as_bool() != Some(true)
            || *active_player_id != source.controller;
        let controller_turn_matches = event["duringControllerTurn"].as_bool() != Some(true)
            || *active_player_id == source.controller;
        let own_zone_matches = event["fromOwnZone"].as_bool() != Some(true)
            || record.owner == Some(source.controller.as_str());
        let ordinal_matches = integer_value(&event["spellCastOrdinal"]).is_none_or(|ordinal| {
            self.cast_turn_ordinal(casting_player_id, &event["where"]) == ordinal.max(0) as usize
        });
        let starting_deck_matches = event["fromStartingDeck"].as_bool().is_none_or(|expected| {
            let conjured = cast_decisions
                .get("conjured")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            expected != conjured
        });
        let prepared_matches = event["preparedOnly"].as_bool() != Some(true)
            || cast_decisions
                .get("preparedSourceInstanceId")
                .and_then(Value::as_str)
                .is_some()
            || self
                .state
                .stack
                .iter()
                .rev()
                .find(|object| object.card.definition.id == spell.id)
                .is_some_and(|object| object.card.flags.get("prepared").copied().unwrap_or(false));
        let parity_matches =
            event["manaValueParityDecisionId"]
                .as_str()
                .is_none_or(|decision_id| {
                    stored_decision_value(&source.definition, decision_id).is_some_and(|parity| {
                        let value = mana_value_with_decisions(spell, cast_decisions);
                        parity.eq_ignore_ascii_case("even") == (value % 2 == 0)
                    })
                });
        let color_matches = event["colorDecisionId"].as_str().is_none_or(|decision_id| {
            stored_decision_value(&source.definition, decision_id)
                .and_then(|color| {
                    let upper = color.to_ascii_uppercase();
                    matches!(upper.as_str(), "W" | "U" | "B" | "R" | "G")
                        .then_some(upper)
                        .or_else(|| color_name_to_symbol(color).map(ToOwned::to_owned))
                })
                .is_some_and(|color| card_color_symbols(spell).contains(&color))
        });
        let creature_type_matches =
            event["creatureTypeDecisionId"]
                .as_str()
                .is_none_or(|decision_id| {
                    Self::chosen_creature_type_matches(
                        source,
                        spell,
                        &json!({ "decisionId": decision_id }),
                    )
                });
        let single_target_matches = event["singleTarget"]
            .as_bool()
            .is_none_or(|required| !required || record.targets.len() == 1);
        let mana_symbol_matches = event["manaSymbol"].as_str().is_none_or(|required_symbol| {
            let count = parse_mana_symbols(&spell.mana_cost)
                .iter()
                .filter(|symbol| symbol.split('/').any(|part| part == required_symbol))
                .count();
            integer_value(&event["minimumManaSymbols"])
                .and_then(|minimum| usize::try_from(minimum).ok())
                .is_some_and(|minimum| count >= minimum)
        });
        let caster_matches = event["anyPlayer"].as_bool() == Some(true)
            || (event["opponentOfSourceController"].as_bool() == Some(true)
                && source.controller != casting_player_id)
            || (event["opponentOfSourceController"].as_bool() != Some(true)
                && event["anyPlayer"].as_bool() != Some(true)
                && source.controller == casting_player_id);
        let targets_match = event
            .get("targets")
            .is_none_or(|targets| self.cast_targets_match(targets, record, source));
        targets_match
            && own_zone_matches
            && from_zone_matches
            && turn_matches
            && controller_turn_matches
            && ordinal_matches
            && starting_deck_matches
            && prepared_matches
            && parity_matches
            && color_matches
            && creature_type_matches
            && single_target_matches
            && mana_symbol_matches
            && caster_matches
            && self.cast_spell_matches(&event["where"], record)
    }
}

/// The record flag of a promised gift (CR 702.174a).
const GIFT_FLAG: &str = "castChoice:gift";
/// The record flag of a paid teamwork cost (CR 702.194).
const TEAMWORK_FLAG: &str = "castChoice:teamwork";
/// The record flag of a spell cast with its escape permission (CR 702.138c).
const ESCAPE_FLAG: &str = "castChoice:escape";

/// `castChoiceMade { spell: self, choice, mode?, manaCost? }`: `gift`,
/// `teamwork`, or `alternativeCost` with exactly one of a casting `mode`
/// and a printed `manaCost`.
pub(super) fn cast_choice_supported(condition: &Value) -> bool {
    value_kind(&condition["spell"]) == Some("self")
        && match condition["choice"].as_str() {
            Some("gift" | "teamwork" | "escape") => {
                condition.get("mode").is_none() && condition.get("manaCost").is_none()
            }
            Some("alternativeCost") => {
                condition["mode"].is_string() != condition["manaCost"].is_string()
                    && condition["manaCost"]
                        .as_str()
                        .is_none_or(|cost| !parse_mana_symbols(cost).is_empty())
            }
            _ => false,
        }
}

/// A mana cost as the multiset of its symbols, so that the printed
/// condition and the paid alternative cost compare by symbols.
fn mana_cost_key(cost: &str) -> String {
    let mut symbols = parse_mana_symbols(cost);
    symbols.sort();
    symbols.join(",")
}

/// The record flag a `castChoiceMade` condition asks for. A casting mode is
/// the follow-up flag the permanent already keeps
/// (`castWithAlternativeCost:MODE`).
pub(super) fn cast_choice_flag(condition: &Value) -> Option<String> {
    match condition["choice"].as_str()? {
        "gift" => Some(GIFT_FLAG.to_string()),
        "teamwork" => Some(TEAMWORK_FLAG.to_string()),
        "escape" => Some(ESCAPE_FLAG.to_string()),
        "alternativeCost" => match condition["mode"].as_str() {
            Some(mode) => Some(format!("castWithAlternativeCost:{mode}")),
            None => condition["manaCost"]
                .as_str()
                .map(|cost| format!("castWithAlternativeManaCost:{}", mana_cost_key(cost))),
        },
        _ => None,
    }
}

impl GameEngine {
    /// The choices the cast decisions of a spell record (CR 601.2b-f), as
    /// the flags the permanent it becomes keeps: a promised gift, a paid
    /// teamwork cost, the casting mode and the mana cost of a paid
    /// alternative cost. A copy of the spell carries the same decisions
    /// (CR 707.10).
    pub(super) fn cast_choice_flags(
        &self,
        definition: &CardDefinition,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        let mut flags = Vec::new();
        if decisions.get("giftRecipient").is_some_and(Value::is_string) {
            flags.push(GIFT_FLAG.to_string());
        }
        if decisions.get("teamworkPaid").and_then(Value::as_bool) == Some(true) {
            flags.push(TEAMWORK_FLAG.to_string());
        }
        if decisions.get("useEscape").and_then(Value::as_bool) == Some(true) {
            flags.push(ESCAPE_FLAG.to_string());
        }
        if decisions.get("useAlternativeCost").and_then(Value::as_bool) == Some(true) {
            if let Some(mode) = decisions.get("alternativeCostMode").and_then(Value::as_str) {
                flags.push(format!("castWithAlternativeCost:{mode}"));
            }
            if let Some(ability) = self.selected_alternative_cost(definition, decisions) {
                for cost in ability["costs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|cost| value_kind(cost) == Some("payMana"))
                {
                    if let Some(mana) = cost["manaCost"].as_str() {
                        flags.push(format!(
                            "castWithAlternativeManaCost:{}",
                            mana_cost_key(mana)
                        ));
                    }
                }
            }
        }
        flags
    }
}
