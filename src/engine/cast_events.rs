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

/// One spell cast.
pub(super) struct CastEventRecord<'a> {
    pub caster: &'a str,
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

impl GameEngine {
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
        let active_player_id = &self.state.players[self.state.active_player].id;
        let turn_matches = event["duringOpponentTurn"].as_bool() != Some(true)
            || (source.controller == casting_player_id && active_player_id != casting_player_id);
        let controller_turn_matches = event["duringControllerTurn"].as_bool() != Some(true)
            || (source.controller == casting_player_id && active_player_id == casting_player_id);
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
        from_zone_matches
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
            && matches_card_filter(spell, &event["where"])
    }
}
