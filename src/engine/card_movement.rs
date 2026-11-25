//! Rules shared by physical card movements between zones.

use super::{CardInstance, EngineError, GameEngine};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum PrivateZoneDestination {
    Hand,
    Graveyard,
    LibraryTop,
    LibraryBottom,
}

/// The finishing-step record of a resolving spell its own effect moved into
/// its owner's library (CR 608.2n).
const RESOLVING_SPELL_TO_LIBRARY: &str = "resolvingSpellToLibrary";

pub(super) struct PrivateZoneReceipt {
    pub card_id: String,
    pub owner_id: String,
    pub actual_zone: &'static str,
}

impl GameEngine {
    /// Moves a card already in a library down so that `cards_above` cards
    /// stay above it (CR 401.2: `ORDINAL from the top`, `just beneath the top
    /// N cards`). A library's top is its last element; with fewer cards
    /// above it the card is at the bottom. `false` when the card is absent.
    pub(super) fn place_library_card_beneath_top(
        &mut self,
        player_index: usize,
        card_id: &str,
        cards_above: usize,
    ) -> bool {
        let Some(player) = self.state.players.get_mut(player_index) else {
            return false;
        };
        let Some(position) = player
            .library
            .iter()
            .rposition(|card| card.instance_id == card_id)
        else {
            return false;
        };
        let card = player.library.remove(position);
        let index = player.library.len().saturating_sub(cards_above);
        player.library.insert(index, card);
        true
    }

    pub(super) fn put_owned_card_into_exile(
        &mut self,
        storage_player_index: usize,
        mut moved: super::card_zone_transition::OwnedMovementCard,
    ) -> Result<(), EngineError> {
        if storage_player_index >= self.state.players.len() {
            return Err(EngineError::new("invalid exile storage player"));
        }
        self.commit_movement_identity(
            &mut moved,
            &super::card_zone_transition::ObjectZone::Exile,
            super::card_zone_transition::ObjectTransitionReason::Reexile,
        )?;
        self.state.players[storage_player_index]
            .exile
            .push(moved.card);
        Ok(())
    }

    pub(super) fn put_known_source_card_into_graveyard(
        &mut self,
        owner_index: usize,
        card: CardInstance,
        source: &super::card_zone_transition::CardMovementSource,
    ) -> Result<bool, EngineError> {
        let plan = self.prepare_graveyard_destination(owner_index, &card);
        self.put_owned_card_into_prepared_graveyard(
            owner_index,
            super::card_zone_transition::OwnedMovementCard {
                source: source.clone(),
                card,
            },
            plan,
        )
    }

    pub(super) fn put_owned_card_into_prepared_graveyard(
        &mut self,
        owner_index: usize,
        mut moved: super::card_zone_transition::OwnedMovementCard,
        plan: super::card_zone_transition::PreparedGraveyardDestination,
    ) -> Result<bool, EngineError> {
        use super::card_zone_transition::{ObjectTransitionReason, ObjectZone};
        if self.player_index(&moved.card.owner)? != owner_index {
            return Err(EngineError::new(
                "graveyard destination must belong to the card owner",
            ));
        }
        if plan.source_card_id() != moved.card.instance_id {
            return Err(EngineError::new(
                "prepared graveyard policy belongs to a different source card",
            ));
        }
        self.validate_prepared_graveyard_destination(&plan)?;
        let actual = plan.actual_zone(&moved.card.owner);
        let reason = if matches!(actual, ObjectZone::Exile) {
            ObjectTransitionReason::Reexile
        } else {
            ObjectTransitionReason::Move
        };
        self.commit_movement_identity(&mut moved, &actual, reason)?;
        self.commit_graveyard_destination(owner_index, moved.card, plan)
    }

    /// CR400.3 applies to the physical destination independently of the
    /// instruction's requested player, who must already have been resolved.
    /// Input order is retained separately for each owner's library subsequence.
    pub(super) fn put_cards_in_owners_private_zones(
        &mut self,
        cards: Vec<CardInstance>,
        destination: PrivateZoneDestination,
    ) -> Result<Vec<PrivateZoneReceipt>, EngineError> {
        let prepared = cards
            .into_iter()
            .map(|card| self.player_index(&card.owner).map(|index| (index, card)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut receipts = Vec::with_capacity(prepared.len());
        let mut libraries: BTreeMap<usize, Vec<CardInstance>> = BTreeMap::new();
        for (index, mut card) in prepared {
            card.controller = card.owner.clone();
            let id = card.instance_id.clone();
            let owner = card.owner.clone();
            let actual_zone = match destination {
                PrivateZoneDestination::Hand => {
                    self.state.players[index].hand.push(card);
                    "hand"
                }
                PrivateZoneDestination::Graveyard => {
                    #[cfg(test)]
                    {
                        if self.put_card_into_graveyard(index, card) {
                            "graveyard"
                        } else {
                            "exile"
                        }
                    }
                    #[cfg(not(test))]
                    {
                        return Err(EngineError::new(
                            "raw graveyard placement requires an explicit prepared transition",
                        ));
                    }
                }
                PrivateZoneDestination::LibraryTop | PrivateZoneDestination::LibraryBottom => {
                    libraries.entry(index).or_default().push(card);
                    "library"
                }
            };
            receipts.push(PrivateZoneReceipt {
                card_id: id,
                owner_id: owner,
                actual_zone,
            });
        }
        for (index, mut cards) in libraries {
            match destination {
                PrivateZoneDestination::LibraryTop => {
                    cards.reverse();
                    self.state.players[index].library.extend(cards);
                }
                PrivateZoneDestination::LibraryBottom => {
                    self.state.players[index].library.splice(0..0, cards);
                }
                _ => {}
            }
        }
        Ok(receipts)
    }

    /// The typed source survives extraction. Resolve replacement before
    /// committing a new incarnation; then reuse ordinary owner/order placement.
    pub(super) fn put_owned_cards_in_owners_private_zones(
        &mut self,
        cards: Vec<super::card_zone_transition::OwnedMovementCard>,
        destination: PrivateZoneDestination,
    ) -> Result<Vec<PrivateZoneReceipt>, EngineError> {
        use super::card_zone_transition::{ObjectTransitionReason, ObjectZone};
        for card in &cards {
            self.player_index(&card.card.owner)?;
        }
        if matches!(destination, PrivateZoneDestination::Graveyard) {
            return Err(EngineError::new(
                "graveyard transition requires policy prepared before source extraction",
            ));
        }
        let mut prepared = Vec::with_capacity(cards.len());
        for mut moved in cards {
            let actual = match destination {
                PrivateZoneDestination::Hand => ObjectZone::Hand(moved.card.owner.clone()),
                PrivateZoneDestination::LibraryTop | PrivateZoneDestination::LibraryBottom => {
                    ObjectZone::Library(moved.card.owner.clone())
                }
                PrivateZoneDestination::Graveyard => unreachable!("graveyard handled above"),
            };
            let reason = if matches!(actual, ObjectZone::Library(_)) && moved.source.zone == actual
            {
                ObjectTransitionReason::Reorder
            } else {
                ObjectTransitionReason::Move
            };
            self.commit_movement_identity(&mut moved, &actual, reason)?;
            prepared.push(moved.card);
        }
        self.put_cards_in_owners_private_zones(prepared, destination)
    }

    /// CR 608.2n: an instruction that moves the resolving spell itself
    /// (`Exile NAME.`, `Shuffle NAME into its owner's library.`) replaces
    /// the spell's own move to its owner's graveyard as it finishes
    /// resolving. The card is on the stack, in no card zone this movement
    /// can take it from, so the move is recorded for the finishing step and
    /// the remaining objects are moved as usual. A copy is never moved
    /// (CR 707.10a): it ceases to exist as it finishes resolving.
    pub(super) fn claim_resolving_spell_movement(
        &mut self,
        destination: &serde_json::Value,
        stack_object: &super::StackObject,
        objects: Vec<String>,
    ) -> Vec<String> {
        let spell_id = stack_object.card.instance_id.as_str();
        let spell_is_moved = super::stack_object_is_spell(stack_object)
            && !super::is_spell_copy(&stack_object.card)
            && objects.iter().any(|object| object == spell_id)
            && self
                .prepare_card_movement_source(spell_id, None)
                .ok()
                .flatten()
                .is_none();
        if !spell_is_moved {
            return objects;
        }
        let modifier = match super::value_kind(destination) {
            Some("exile") if destination["faceDown"].as_bool() != Some(true) => {
                let mut modifier =
                    serde_json::json!({"kind":"exileResolvingSpell", "cardInstanceId":spell_id});
                // `Exile NAME with N KIND counters on it` (CR 702.62a): the
                // counted placements go with the card into exile.
                if let Some(counters) = destination.get("enterWithCounters") {
                    modifier["counters"] = counters.clone();
                }
                Some(modifier)
            }
            Some("library") => Some(
                serde_json::json!({"kind":RESOLVING_SPELL_TO_LIBRARY, "cardInstanceId":spell_id}),
            ),
            _ => None,
        };
        let Some(modifier) = modifier else {
            return objects;
        };
        self.state.rule_modifiers.push(modifier);
        objects
            .into_iter()
            .filter(|object| object != spell_id)
            .collect()
    }

    /// Whether an effect of the resolving spell moved that spell into its
    /// owner's library (CR 608.2n); the record is consumed. The finishing
    /// step then shuffles it into that library (CR 701.24a).
    pub(super) fn take_resolving_spell_library_move(&mut self, spell_id: &str) -> bool {
        let before = self.state.rule_modifiers.len();
        self.state.rule_modifiers.retain(|modifier| {
            super::value_kind(modifier) != Some(RESOLVING_SPELL_TO_LIBRARY)
                || modifier["cardInstanceId"].as_str() != Some(spell_id)
        });
        self.state.rule_modifiers.len() != before
    }

    /// CR111.8: after a token has left the battlefield it cannot change zones
    /// again, even before the next state-based action makes it cease to exist.
    pub(super) fn nonbattlefield_card_can_change_zone(&self, instance_id: &str) -> bool {
        self.state
            .players
            .iter()
            .flat_map(|player| {
                [
                    &player.library,
                    &player.hand,
                    &player.graveyard,
                    &player.exile,
                    &player.sideboard,
                    &player.command_zone,
                ]
                .into_iter()
                .flat_map(|zone| zone.iter())
            })
            .find(|card| card.instance_id == instance_id)
            .is_none_or(|card| !card.definition.is_token)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "card_movement_owner_tests.rs"]
mod owner_tests;

/// The counters an `exileResolvingSpell` record puts on the card as it is
/// exiled: one `addCounter`, and each counted `counters` placement.
pub(super) fn resolving_spell_exile_counters(modifier: &serde_json::Value) -> Vec<(String, i32)> {
    let mut counters = modifier["addCounter"]
        .as_str()
        .map(|counter| vec![(counter.to_string(), 1)])
        .unwrap_or_default();
    counters.extend(
        modifier["counters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|counter| {
                let name = counter["counter"].as_str()?;
                let count = counter["count"]
                    .as_i64()
                    .and_then(|count| i32::try_from(count).ok())?;
                (count > 0).then(|| (name.to_string(), count))
            }),
    );
    counters
}
