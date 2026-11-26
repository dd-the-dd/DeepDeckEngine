//! Zone of function (CR 113.6, 604.3): where a rule of an object functions.
//!
//! A static ability states it with `activeWhile: inZone(self, zone)`; a
//! triggered ability with `triggerZone` (absent means the battlefield,
//! CR 113.6). Every observer that asks "does this rule function from where
//! its object is" reads one typed value instead of re-matching JSON shapes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneOfFunction {
    Battlefield,
    Graveyard,
    Hand,
    Exile,
    Library,
    Stack,
    /// While the object is a spell, or being cast (CR 601.2f reductions).
    StackOrCast,
    Command,
    CommandZoneOrBattlefield,
}

impl ZoneOfFunction {
    fn from_static_zone(zone: &str) -> Option<Self> {
        Some(match zone {
            "battlefield" => Self::Battlefield,
            "graveyard" => Self::Graveyard,
            "hand" => Self::Hand,
            "exile" => Self::Exile,
            "library" => Self::Library,
            "stack" => Self::Stack,
            "stackOrCast" => Self::StackOrCast,
            "command" => Self::Command,
            "commandZoneOrBattlefield" => Self::CommandZoneOrBattlefield,
            _ => return None,
        })
    }

    /// Whether a rule with this zone of function functions while its object
    /// is in `zone` (engine zone names: battlefield, graveyard, hand, exile,
    /// library, stack, commandZone).
    pub(super) fn functions_in(self, zone: &str) -> bool {
        match self {
            Self::Battlefield => zone == "battlefield",
            Self::Graveyard => zone == "graveyard",
            Self::Hand => zone == "hand",
            Self::Exile => zone == "exile",
            Self::Library => zone == "library",
            Self::Stack | Self::StackOrCast => zone == "stack",
            Self::Command => zone == "commandZone",
            Self::CommandZoneOrBattlefield => matches!(zone, "commandZone" | "battlefield"),
        }
    }

    /// Zones other than the battlefield whose static abilities feed the
    /// continuous-effect collector (CR 604.3: they say where they function).
    pub(super) fn is_card_zone_outside_battlefield(self) -> bool {
        matches!(self, Self::Graveyard | Self::Hand | Self::Exile)
    }
}

/// The zone of function a static rule states, if it is a self `inZone`.
pub(super) fn static_zone_of_function(rule: &Value) -> Option<ZoneOfFunction> {
    let active = &rule["activeWhile"];
    if value_kind(active) != Some("inZone") || value_kind(&active["object"]) != Some("self") {
        return None;
    }
    value_kind(&active["zone"]).and_then(ZoneOfFunction::from_static_zone)
}

/// The zone of function of a triggered rule. Absent means the battlefield.
pub(super) fn trigger_zone_of_function(rule: &Value) -> ZoneOfFunction {
    match rule["triggerZone"].as_str() {
        None => ZoneOfFunction::Battlefield,
        Some(zone) => ZoneOfFunction::from_static_zone(zone).unwrap_or(ZoneOfFunction::Battlefield),
    }
}

/// Whether a printed rule is a triggered ability whose zone of function
/// excludes the battlefield (CR 113.6: `triggerZone` graveyard, hand or
/// exile). Such a rule does not function on a permanent.
pub(super) fn functions_only_off_battlefield(rule: &Value) -> bool {
    value_kind(rule) == Some("triggeredAbility")
        && rule.get("triggerZone").is_some()
        && !trigger_zone_of_function(rule).functions_in("battlefield")
}

/// Event kinds whose publication also reads the triggered rules of cards in
/// a graveyard (CR 113.6), each through the matcher its battlefield
/// observers use: a step beginning, a spell cast by a player, a card drawn,
/// a life gain, an attack declaration batch and the player outcome events of
/// one publication routine.
const GRAVEYARD_OBSERVED_EVENT_KINDS: [&str; 11] = [
    "stepBegan",
    "spellCast",
    "cardDrawn",
    "lifeGained",
    "controlledCreaturesAttacked",
    "crimeCommitted",
    "proliferated",
    "dieRolled",
    "coinFlipped",
    "clashed",
    "permanentExerted",
];

/// Whether a card in a graveyard observes one event alternative: an event
/// kind of [`GRAVEYARD_OBSERVED_EVENT_KINDS`] (a spell's own cast is
/// observed on the stack, not from a graveyard), or a zone change read by
/// the entry, death or graveyard-arrival publication. A departure observed
/// only by captured battlefield sources is not.
pub(super) fn graveyard_event_observed(event: &Value) -> bool {
    match value_kind(event) {
        Some("spellCast") => cast_events::cast_subject(event) == cast_events::CastSubject::AnySpell,
        Some(kind) if GRAVEYARD_OBSERVED_EVENT_KINDS.contains(&kind) => true,
        _ => zone_change_events::zone_change_pattern(event)
            .and_then(Result::ok)
            .is_some_and(|pattern| {
                matches!(
                    pattern.group,
                    zone_change_events::ZoneChangeGroup::Entry
                        | zone_change_events::ZoneChangeGroup::Death
                        | zone_change_events::ZoneChangeGroup::GraveyardArrival
                )
            }),
    }
}

/// Whether the zone a triggered rule declares has a runtime observer of
/// every one of its event alternatives. The battlefield and the zones with
/// their own observers (exile for suspend-like rules, the hand at the first
/// upkeep, the command zone) keep their established admission; a graveyard
/// rule is admitted only for events a graveyard card observes.
pub(super) fn trigger_zone_observed(rule: &Value) -> bool {
    match trigger_zone_of_function(rule) {
        ZoneOfFunction::Graveyard => triggered_event_variants(&rule["event"])
            .into_iter()
            .all(graveyard_event_observed),
        _ => true,
    }
}

/// A printed rule of a card outside the battlefield that observes events
/// from `zone`: a supported triggered rule whose zone of function is that
/// zone, with its metadata read off the battlefield.
fn card_zone_trigger_rule(rule: &Value, zone: &str) -> bool {
    functions_only_off_battlefield(rule)
        && trigger_zone_of_function(rule).functions_in(zone)
        && triggered_rule_supported(rule)
        && GameEngine::rule_metadata_active_off_battlefield(rule)
}

/// A static ability that functions from a card zone other than the
/// battlefield ("as long as this card is in your graveyard"). It shares the
/// battlefield modifier vocabulary; only its zone of function differs.
pub(super) fn card_zone_static_rule_supported(rule: &Value) -> bool {
    static_zone_of_function(rule).is_some_and(ZoneOfFunction::is_card_zone_outside_battlefield)
        && rule.get("condition").is_none_or(condition_supported)
        && rule["modifiers"].as_array().is_some_and(|modifiers| {
            !modifiers.is_empty() && modifiers.iter().all(battlefield_static_modifier_supported)
        })
}

/// An activation limit of an ability activated from its owner's hand
/// (forecast, CR 702.57a-b: `only once each turn`). The use is recorded on
/// the card in hand, which stays there when its cost only reveals it; a card
/// that leaves and returns is a new object with no use (CR 400.7). Another
/// limit or another off-battlefield zone keeps no use record and is refused.
pub(super) fn hand_activation_limit_supported(rule: &Value) -> bool {
    rule["activationZone"].as_str() == Some("hand")
        && value_kind(&rule["activationLimit"]) == Some("oncePerTurn")
        && rule["activationLimit"]["id"].is_string()
}

impl GameEngine {
    /// Cards in the graveyards of players still in the game whose printed
    /// triggered rules function there (CR 113.6), as trigger sources. Each
    /// is the card in its zone; "you" of an object outside the battlefield
    /// and the stack is its owner (CR 108.4a, 109.5), so the source carries
    /// its owner as controller. A card that leaves the graveyard before its
    /// trigger resolves is a new object the program's `this card` no longer
    /// finds (CR 400.7).
    pub(super) fn graveyard_trigger_sources(&self) -> Vec<(CardInstance, &'static str)> {
        self.state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.graveyard.iter())
            .filter(|card| {
                card.definition
                    .printed_rules()
                    .iter()
                    .any(|rule| card_zone_trigger_rule(rule, "graveyard"))
            })
            .map(|card| {
                let mut source = card.clone();
                source.controller = source.owner.clone();
                (source, "graveyard")
            })
            .collect()
    }

    /// The triggered rules of graveyard trigger sources (see
    /// [`Self::graveyard_trigger_sources`]) that `observes` one occurrence,
    /// read with the matcher of the publishing event family.
    pub(super) fn card_zone_triggered_rules(
        &self,
        observes: impl Fn(&CardInstance, &Value) -> bool,
    ) -> Vec<(CardInstance, Value)> {
        let mut triggers = Vec::new();
        for (source, zone) in self.graveyard_trigger_sources() {
            for rule in source
                .definition
                .printed_rules()
                .iter()
                .filter(|rule| card_zone_trigger_rule(rule, zone) && observes(&source, *rule))
            {
                triggers.push((source.clone(), rule.clone()));
            }
        }
        triggers
    }

    /// Puts graveyard triggers on the stack with the occurrence's decisions.
    /// Returns `false` once a quantity failure stops the publication.
    pub(super) fn publish_card_zone_triggers(
        &mut self,
        triggers: Vec<(CardInstance, Value)>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return false;
            }
        }
        true
    }

    /// Spell-cast observers in graveyards (`Whenever you cast a spell, ...
    /// return this card from your graveyard`), matched on the one cast
    /// record by the shared cast matcher (CR 601.2i).
    pub(super) fn graveyard_spell_cast_triggers(
        &self,
        record: &cast_events::CastEventRecord<'_>,
    ) -> Vec<(CardInstance, Value, trigger_multiplicity::TriggerOrigin)> {
        self.card_zone_triggered_rules(|source, rule| {
            triggered_event_variants(&rule["event"])
                .into_iter()
                .any(|event| {
                    self.spell_cast_matches(
                        event,
                        record,
                        source,
                        cast_events::CastSubject::AnySpell,
                    )
                })
        })
        .into_iter()
        .map(|(source, rule)| {
            (
                source,
                rule,
                trigger_multiplicity::TriggerOrigin::ObjectAbility,
            )
        })
        .collect()
    }

    /// Attack-declaration observers in graveyards (`Whenever you attack with
    /// three or more creatures, ... return this card from your graveyard`),
    /// matched by the attack batch matcher of battlefield observers.
    pub(super) fn graveyard_attack_batch_triggers(
        &self,
        attacker_ids: &[String],
    ) -> Vec<(CardInstance, Value, trigger_multiplicity::TriggerOrigin)> {
        self.card_zone_triggered_rules(|source, rule| {
            triggered_event_variants(&rule["event"])
                .into_iter()
                .any(|event| {
                    value_kind(event) == Some("controlledCreaturesAttacked")
                        && self.attack_batch_observed(event, source, attacker_ids)
                })
        })
        .into_iter()
        .map(|(source, rule)| {
            (
                source,
                rule,
                trigger_multiplicity::TriggerOrigin::ObjectAbility,
            )
        })
        .collect()
    }

    /// Card-drawn observers in graveyards, matched by the draw matcher of
    /// battlefield observers.
    pub(super) fn graveyard_card_drawn_triggers(
        &self,
        drawing_player_id: &str,
    ) -> Vec<(CardInstance, Value)> {
        self.card_zone_triggered_rules(|source, rule| {
            self.card_drawn_rule_observed(rule, source, drawing_player_id)
        })
    }

    /// Life-gain observers in graveyards (`Whenever an opponent gains
    /// life, ... return this card from your graveyard`), with the gaining
    /// player matched by the shared event player relation.
    pub(super) fn enqueue_graveyard_life_gained_triggers(&mut self, player_id: &str, amount: i32) {
        let triggers = self.card_zone_triggered_rules(|source, rule| {
            triggered_event_variants(&rule["event"])
                .into_iter()
                .any(|event| {
                    value_kind(event) == Some("lifeGained")
                        && self.event_player_reference_matches(&event["player"], source, player_id)
                })
        });
        self.publish_card_zone_triggers(
            triggers,
            &BTreeMap::from([
                (
                    "triggeringPlayerId".to_string(),
                    Value::String(player_id.to_string()),
                ),
                ("lifeGainedAmount".to_string(), Value::from(amount)),
            ]),
        );
    }

    /// Death observers in graveyards (`Whenever a creature you control
    /// dies, if this card is in your graveyard`), matched on the death
    /// record. The dead card is not an observer of its own death from the
    /// graveyard it arrives in.
    pub(super) fn graveyard_death_triggers(
        &self,
        record: &zone_change_events::ZoneChangeRecord<'_>,
    ) -> Vec<(CardInstance, Value)> {
        self.card_zone_triggered_rules(|source, rule| {
            source.instance_id != record.object.instance_id
                && self.rule_matches_zone_change(
                    rule,
                    zone_change_events::ZoneChangeGroup::Death,
                    record,
                    source,
                )
        })
    }

    /// Graveyard-arrival observers in graveyards (`Whenever a nontoken
    /// creature is put into your graveyard from the battlefield, if this
    /// card is in your graveyard`), with the arriving cards each observes.
    /// A card of the arriving batch is not yet an observer of that batch.
    pub(super) fn graveyard_arrival_observers(
        &self,
        cards: &[CardInstance],
        from: Option<&'static str>,
    ) -> Vec<(CardInstance, Value, Vec<Value>)> {
        let mut observers = Vec::new();
        for (source, rule) in self.card_zone_triggered_rules(|source, _| {
            !cards
                .iter()
                .any(|card| card.instance_id == source.instance_id)
        }) {
            let matching = cards
                .iter()
                .filter(|card| {
                    self.rule_matches_zone_change(
                        &rule,
                        zone_change_events::ZoneChangeGroup::GraveyardArrival,
                        &zone_change_events::ZoneChangeRecord {
                            object: card,
                            from,
                            to: "graveyard",
                            reason: None,
                        },
                        &source,
                    )
                })
                .map(|card| Value::String(card.instance_id.clone()))
                .collect::<Vec<_>>();
            if !matching.is_empty() {
                observers.push((source, rule, matching));
            }
        }
        observers
    }

    /// Records one use of a hand-activated ability's limit on the card in
    /// hand (see [`hand_activation_limit_supported`]). A source that left the
    /// hand while its costs were paid keeps no record: it is a new object.
    pub(super) fn record_hand_activation_use(
        &mut self,
        player_index: usize,
        zone_name: &str,
        instance_id: &str,
        rule: &Value,
        rule_index: usize,
    ) {
        if zone_name != "hand" || !hand_activation_limit_supported(rule) {
            return;
        }
        let Some(flag) =
            activation_limit_flag(&rule["activationLimit"], self.state.turn_number, rule_index)
        else {
            return;
        };
        if let Some(card) = self.state.players[player_index]
            .hand
            .iter_mut()
            .find(|card| card.instance_id == instance_id)
        {
            card.flags.insert(flag, true);
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "zone_of_function_tests.rs"]
mod tests;
