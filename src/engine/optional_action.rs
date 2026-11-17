//! Whether an optional instruction was actually performed.
//!
//! CR 608.2c: "If you do" / "If you don't" refer to the instruction itself,
//! and CR 603.12: a reflexive "When you do" triggers only if that action
//! occurred during the resolution. Choosing the instruction is not enough:
//! a replaced event never happens (CR 614.6), and an impossible part of an
//! effect does only as much as possible (CR 609.3). Each measured primitive
//! therefore names a witness that is prepared before the instruction runs
//! and checked against the game events and object locations it produced.
//!
//! Performance definitions (the instruction counts as performed when ...):
//! - `drawCards`: the player drew the full count (CR 121.1). Choosing to draw
//!   from an empty library is allowed (CR 121.3) but draws nothing.
//! - `discardCards`: the player discarded the full count (CR 701.9a).
//! - `discardHand`: every card in the hand when the instruction began was
//!   discarded (CR 701.9a); an empty hand is discarded vacuously.
//! - `mill`: the player milled the full count; a player can't choose to mill
//!   more cards than their library holds (CR 701.17b).
//! - `exileCards` (from a graveyard): the full count was exiled (CR 701.13a).
//! - `sacrificePermanents`: the player sacrificed the full count (CR 701.21a).
//! - `gainLife`: the player gained at least the stated amount; prevented gain
//!   is not gained (CR 119.10, 615).
//! - `createTokens`: at least the stated quantity of tokens was created
//!   (CR 111.1); a replacement may create more.
//! - `putCounters` on one object: at least the stated number of counters was
//!   put on it (CR 122.1); a counter that can't be placed isn't put.
//! - `tapPermanent` / `untapPermanent`: every referenced permanent actually
//!   changed its tapped status (CR 701.26a, 701.26b).
//! - `destroyPermanent`, `exilePermanent`, `sacrificePermanent`,
//!   `returnToOwnersHand`, `moveCards`, `exileTargetCards`: every referenced
//!   object left the zone it occupied when the instruction began (CR 400.7,
//!   701.8a, 701.13a, 701.21a). Regeneration or indestructibility leaves it
//!   in place; a destination replacement still moves it.
use super::*;

/// Static contract: true when `action` is a primitive whose performance this
/// module measures. Any other optional instruction is unsupported rather than
/// treated as performed whenever it is chosen.
pub(super) fn optional_action_witness_supported(action: &Value) -> bool {
    let single_player = |reference: &Value| {
        player_reference_supported(reference)
            && !matches!(
                value_kind(reference),
                Some("opponentsOf" | "eachPlayer" | "chosenTargets")
            )
    };
    match value_kind(action) {
        Some("drawCards" | "mill") => single_player(&action["player"]),
        Some("discardCards") => single_player(&action["player"]) && action.get("cards").is_none(),
        Some("discardHand") => single_player(&action["player"]),
        Some("gainLife") => single_player(&action["player"]),
        Some("sacrificePermanents") => {
            single_player(&action["player"]) && action.get("objects").is_none()
        }
        Some("exileCards") => single_player(&action["from"]["player"]),
        Some("createTokens") => true,
        Some("putCounters") => matches!(
            value_kind(&action["permanent"]),
            Some("self" | "abilitySource" | "chosenTarget")
        ),
        Some(
            "tapPermanent" | "untapPermanent" | "destroyPermanent" | "exilePermanent"
            | "sacrificePermanent",
        ) => action["permanent"].is_object(),
        Some("returnToOwnersHand") => action["object"].is_object(),
        Some("moveCards" | "exileTargetCards") => action["cards"].is_object(),
        _ => false,
    }
}

/// The measurement prepared before the instruction runs.
pub(super) enum PerformanceWitness {
    /// Events of `kinds` (by `player` and on `objects` when given) whose
    /// measured total reaches `required`.
    Events {
        kinds: &'static [&'static str],
        player: Option<String>,
        objects: Option<Vec<String>>,
        measure: EventMeasure,
        required: i64,
    },
    /// Every object left the zone it occupied before the instruction.
    ZoneChange {
        objects: Vec<(String, Option<&'static str>)>,
    },
}

pub(super) enum EventMeasure {
    Count,
    Sum(&'static str),
}

impl GameEngine {
    /// Prepares the witness of a supported instruction. An unresolved
    /// reference yields a witness that can never be satisfied.
    pub(super) fn prepare_optional_action_witness(
        &self,
        action: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> PerformanceWitness {
        let amount = |field: &str| {
            i64::from(
                self.runtime_numeric_expression(&action[field], stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0),
            )
        };
        let player = |reference: &Value| self.resolve_player(reference, stack_object, bindings);
        let events = |kinds: &'static [&'static str],
                      player: Option<String>,
                      measure: EventMeasure,
                      required: i64| {
            // An unresolved actor can't have performed anything.
            match player {
                Some(player) => PerformanceWitness::Events {
                    kinds,
                    player: Some(player),
                    objects: None,
                    measure,
                    required,
                },
                None => PerformanceWitness::ZoneChange {
                    objects: Vec::new(),
                },
            }
        };
        match value_kind(action) {
            Some("drawCards") => events(
                &["cardDrawn"],
                player(&action["player"]),
                EventMeasure::Count,
                amount("count"),
            ),
            Some("discardCards") => events(
                &["cardDiscarded"],
                player(&action["player"]),
                EventMeasure::Count,
                amount("count"),
            ),
            Some("discardHand") => {
                let player = player(&action["player"]);
                let held = player
                    .as_deref()
                    .and_then(|player| self.player_index(player).ok())
                    .map_or(0, |index| self.state.players[index].hand.len() as i64);
                events(&["cardDiscarded"], player, EventMeasure::Count, held)
            }
            Some("mill") => events(
                &["cardMilled"],
                player(&action["player"]),
                EventMeasure::Count,
                amount("count"),
            ),
            Some("exileCards") => events(
                &["cardsExiled"],
                player(&action["from"]["player"]),
                EventMeasure::Sum("count"),
                amount("count"),
            ),
            Some("sacrificePermanents") => events(
                &["permanentSacrificed"],
                player(&action["player"]),
                EventMeasure::Count,
                amount("count"),
            ),
            Some("gainLife") => events(
                &["lifeGained"],
                player(&action["player"]),
                EventMeasure::Sum("amount"),
                amount("amount"),
            ),
            Some("createTokens") => PerformanceWitness::Events {
                kinds: &["tokenCreated"],
                player: None,
                objects: None,
                measure: EventMeasure::Count,
                required: amount("quantity"),
            },
            Some("putCounters") => {
                let objects = self.optional_action_objects(
                    &action["permanent"],
                    stack_object,
                    bindings,
                    decisions,
                );
                let required = amount("count") * objects.len() as i64;
                PerformanceWitness::Events {
                    kinds: &["countersPlaced"],
                    player: None,
                    objects: Some(objects),
                    measure: EventMeasure::Sum("count"),
                    required,
                }
            }
            Some(kind @ ("tapPermanent" | "untapPermanent")) => {
                let objects = self.optional_action_objects(
                    &action["permanent"],
                    stack_object,
                    bindings,
                    decisions,
                );
                let required = objects.len() as i64;
                PerformanceWitness::Events {
                    kinds: if kind == "tapPermanent" {
                        &["permanentTapped"]
                    } else {
                        &["permanentUntapped"]
                    },
                    player: None,
                    objects: Some(objects),
                    measure: EventMeasure::Count,
                    required,
                }
            }
            Some(kind) => {
                let field = match kind {
                    "returnToOwnersHand" => "object",
                    "moveCards" | "exileTargetCards" => "cards",
                    _ => "permanent",
                };
                let objects = self
                    .optional_action_objects(&action[field], stack_object, bindings, decisions)
                    .into_iter()
                    .map(|id| {
                        let zone = self.object_zone_name(&id);
                        (id, zone)
                    })
                    .collect();
                PerformanceWitness::ZoneChange { objects }
            }
            None => PerformanceWitness::ZoneChange {
                objects: Vec::new(),
            },
        }
    }

    /// Checks a prepared witness against the events recorded from
    /// `first_event` on and the objects' current locations.
    pub(super) fn optional_action_was_performed(
        &self,
        witness: &PerformanceWitness,
        first_event: usize,
    ) -> bool {
        match witness {
            PerformanceWitness::Events {
                kinds,
                player,
                objects,
                measure,
                required,
            } => {
                if objects.as_ref().is_some_and(Vec::is_empty) {
                    return false;
                }
                let total: i64 = self.state.events[first_event.min(self.state.events.len())..]
                    .iter()
                    .filter(|event| kinds.contains(&event.kind.as_str()))
                    .filter(|event| {
                        player
                            .as_deref()
                            .is_none_or(|player| event.player_id.as_deref() == Some(player))
                    })
                    .filter(|event| {
                        objects.as_ref().is_none_or(|objects| {
                            event
                                .card_instance_id
                                .as_ref()
                                .is_some_and(|id| objects.contains(id))
                        })
                    })
                    .map(|event| match measure {
                        EventMeasure::Count => 1,
                        EventMeasure::Sum(field) => event.detail[*field].as_i64().unwrap_or(0),
                    })
                    .sum();
                total >= *required
            }
            PerformanceWitness::ZoneChange { objects } => {
                !objects.is_empty()
                    && objects.iter().all(|(id, before)| {
                        before.is_some() && self.object_zone_name(id) != *before
                    })
            }
        }
    }

    /// Instance ids named by an instruction's object reference.
    fn optional_action_objects(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        if let Some(ids) =
            self.resolve_runtime_object_ids(reference, stack_object, bindings, decisions)
            && !ids.is_empty()
        {
            return ids;
        }
        match self.resolve_target(reference, stack_object, bindings) {
            Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                vec![instance_id]
            }
            _ => Vec::new(),
        }
    }

    /// The zone currently holding an object, or `None` once it ceased to exist.
    fn object_zone_name(&self, instance_id: &str) -> Option<&'static str> {
        self.state.players.iter().find_map(|player| {
            let zones: [(&'static str, &Vec<CardInstance>); 7] = [
                ("battlefield", &player.battlefield),
                ("library", &player.library),
                ("hand", &player.hand),
                ("graveyard", &player.graveyard),
                ("exile", &player.exile),
                ("sideboard", &player.sideboard),
                ("commandZone", &player.command_zone),
            ];
            zones.into_iter().find_map(|(name, cards)| {
                cards
                    .iter()
                    .any(|card| card.instance_id == instance_id)
                    .then_some(name)
            })
        })
    }
}
