//! Physical storage is not a Magic zone. A prepared source is read-only;
//! incarnation allocation belongs to the actual destination commitment.

use super::object_identity::{
    IdentityAssignment, ObjectIdentity, existing_object_identity_of, install_object_identities,
    object_identity_of, prepare_object_identities,
};
use super::{CardInstance, EngineError, GameEngine};

pub(super) fn node_requires_checkpoint(value: &serde_json::Value) -> bool {
    matches!(
        super::value_kind(value),
        Some(
            "moveCards"
                | "moveCard"
                | "moveTargetCard"
                | "move"
                | "discardCards"
                | "discardHand"
                | "exileCardsFromHand"
                | "exileCards"
                | "exilePlayerGraveyard"
                | "exileChosenPlayersGraveyards"
                | "exileTogether"
                | "exileAbilitySourceFromGraveyard"
                | "exileTargetCards"
                | "moveTriggeringCardFromGraveyard"
                | "moveAbilitySourceToHand"
                | "returnLinkedExiledCardToHand"
                | "returnAbilitySourceFromGraveyard"
                | "moveTriggeringCard"
                | "drawCards"
                | "mill"
                | "counterSpell"
                | "counterStackObject"
                | "counterOpponentStackObjects"
                | "suspendStackItem"
                | "returnToOwnersHand"
                | "exileFromTopUntil"
                | "createCardCopy"
                | "conjureNamedCard"
                | "createTokenCopy"
        )
    )
}

pub(super) fn stack_requires_checkpoint(object: &super::StackObject) -> bool {
    super::stack_object_is_spell(object) && !super::is_spell_copy(&object.card)
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum ObjectZone {
    Library(String),
    Hand(String),
    Graveyard(String),
    Battlefield,
    Exile,
    Command,
    Stack,
    /// A card its owner keeps outside the game (CR 400.11). It is physical
    /// storage, not a game object: it has no incarnation until an effect
    /// brings it into the game, where it is admitted as a new object.
    OutsideGame(String),
}

impl ObjectZone {
    pub(super) fn owner(&self) -> Option<&str> {
        match self {
            Self::Library(owner)
            | Self::Hand(owner)
            | Self::Graveyard(owner)
            | Self::OutsideGame(owner) => Some(owner),
            Self::Battlefield | Self::Exile | Self::Command | Self::Stack => None,
        }
    }

    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Library(_) => "library",
            Self::Hand(_) => "hand",
            Self::Graveyard(_) => "graveyard",
            Self::Battlefield => "battlefield",
            Self::Exile => "exile",
            Self::Command => "commandZone",
            Self::Stack => "stack",
            Self::OutsideGame(_) => "outsideGame",
        }
    }

    pub(super) fn is_card_target_zone(&self) -> bool {
        // CR 400.11: cards outside the game are never targetable.
        !matches!(self, Self::Battlefield | Self::Stack | Self::OutsideGame(_))
    }
}

#[derive(Clone, Debug)]
pub(super) struct CardMovementSource {
    pub zone: ObjectZone,
    pub storage_player_id: String,
    pub identity: ObjectIdentity,
}

pub(super) struct OwnedMovementCard {
    pub source: CardMovementSource,
    pub card: CardInstance,
}

impl std::ops::Deref for OwnedMovementCard {
    type Target = CardInstance;

    fn deref(&self) -> &Self::Target {
        &self.card
    }
}

impl std::ops::DerefMut for OwnedMovementCard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.card
    }
}

/// Reordering is not a new-object instruction. The same-zone exceptions in
/// CR400.8-400.10 must be represented by the instruction, not by zone equality.
#[derive(Clone, Copy)]
pub(super) enum ObjectTransitionReason {
    Move,
    Reorder,
    Reexile,
    CommandNewObject,
}

impl GameEngine {
    pub(super) fn prepare_card_graveyard_destination(
        &self,
        instance_id: &str,
    ) -> Result<PreparedGraveyardDestination, EngineError> {
        let source = self
            .prepare_card_movement_source(instance_id, None)?
            .ok_or_else(|| EngineError::new("graveyard movement source is missing"))?;
        let card = if source.zone == ObjectZone::Battlefield {
            self.permanent_position(instance_id)
                .map(|(player, index)| &self.state.players[player].battlefield[index])
        } else {
            self.card_outside_battlefield(instance_id)
        }
        .ok_or_else(|| EngineError::new("prepared graveyard source disappeared"))?;
        let owner = self.player_index(&card.owner)?;
        Ok(self.prepare_graveyard_destination(owner, card))
    }

    pub(super) fn take_zone_card_for_graveyard(
        &mut self,
        player_index: usize,
        zone: &str,
        card_index: usize,
    ) -> Result<(OwnedMovementCard, PreparedGraveyardDestination), EngineError> {
        let player = self
            .state
            .players
            .get(player_index)
            .ok_or_else(|| EngineError::new("invalid graveyard movement source player"))?;
        let cards = match zone {
            "library" => &player.library,
            "hand" => &player.hand,
            "graveyard" => &player.graveyard,
            "exile" => &player.exile,
            "commandZone" => &player.command_zone,
            _ => {
                return Err(EngineError::new(
                    "unsupported graveyard movement source zone",
                ));
            }
        };
        let id = cards
            .get(card_index)
            .ok_or_else(|| EngineError::new("missing graveyard movement source slot"))?
            .instance_id
            .clone();
        let plan = self.prepare_card_graveyard_destination(&id)?;
        Ok((
            self.take_zone_card_for_movement(player_index, zone, card_index)?,
            plan,
        ))
    }

    /// New physical carriers are admitted at construction/publication, never
    /// by a reference read or as recovery for an existing carrier.
    pub(super) fn admit_new_card_carrier(
        &mut self,
        card: &mut CardInstance,
    ) -> Result<(), EngineError> {
        let prepared = prepare_object_identities(
            self.state.battlefield_incarnation_sequence,
            &[&*card],
            IdentityAssignment::Admission,
        )?;
        install_object_identities(
            &mut self.state.battlefield_incarnation_sequence,
            &mut [card],
            prepared,
        )?;
        Ok(())
    }

    /// The parent owns the pop/stack lifecycle. This reads that authenticated
    /// carrier, not a zone lookup that could adopt a later same-ID object.
    pub(super) fn detached_spell_movement_source(
        &self,
        object: &super::StackObject,
    ) -> Result<CardMovementSource, EngineError> {
        if !stack_requires_checkpoint(object) {
            return Err(EngineError::new(
                "physical stack transition requires a noncopy spell",
            ));
        }
        if self
            .prepare_card_movement_source(&object.card.instance_id, None)?
            .is_some()
        {
            return Err(EngineError::new(
                "physical spell carrier is duplicated in a live zone",
            ));
        }
        self.player_index(&object.card.owner)?;
        Ok(CardMovementSource {
            zone: ObjectZone::Stack,
            storage_player_id: object.controller.clone(),
            identity: object_identity_of(&object.card)?,
        })
    }

    pub(super) fn prepare_stack_card_movement_source(
        &self,
        index: usize,
    ) -> Result<CardMovementSource, EngineError> {
        let object = self
            .state
            .stack
            .get(index)
            .ok_or_else(|| EngineError::new("missing stack movement source"))?;
        self.detached_spell_movement_source(object)
    }

    /// Narrow adapter for an already selected physical slot. It authenticates
    /// before removal and delegates extraction to the common source boundary.
    pub(super) fn take_zone_card_for_movement(
        &mut self,
        player_index: usize,
        zone: &str,
        card_index: usize,
    ) -> Result<OwnedMovementCard, EngineError> {
        let player = self
            .state
            .players
            .get(player_index)
            .ok_or_else(|| EngineError::new("invalid movement source player"))?;
        let cards = match zone {
            "library" => &player.library,
            "hand" => &player.hand,
            "graveyard" => &player.graveyard,
            "exile" => &player.exile,
            "commandZone" => &player.command_zone,
            _ => return Err(EngineError::new("unsupported card movement source zone")),
        };
        let id = cards
            .get(card_index)
            .ok_or_else(|| EngineError::new("missing movement source slot"))?
            .instance_id
            .clone();
        let constraint = (zone.to_owned(), player_index);
        let source = self
            .prepare_card_movement_source(&id, Some(&constraint))?
            .ok_or_else(|| EngineError::new("missing selected card movement source"))?;
        self.take_prepared_movement_card(source)
    }

    /// Search every physical live zone before applying a source restriction.
    /// Duplicate IDs and missing admission cannot be hidden by first-match lookup.
    pub(super) fn prepare_card_movement_source(
        &self,
        id: &str,
        constraint: Option<&(String, usize)>,
    ) -> Result<Option<CardMovementSource>, EngineError> {
        let mut found = None;
        for (index, player) in self.state.players.iter().enumerate() {
            for (zone, cards) in [
                (
                    Some(ObjectZone::Library(player.id.clone())),
                    &player.library,
                ),
                (Some(ObjectZone::Hand(player.id.clone())), &player.hand),
                (
                    Some(ObjectZone::Graveyard(player.id.clone())),
                    &player.graveyard,
                ),
                (Some(ObjectZone::Battlefield), &player.battlefield),
                (Some(ObjectZone::Exile), &player.exile),
                (Some(ObjectZone::Command), &player.command_zone),
                (
                    Some(ObjectZone::OutsideGame(player.id.clone())),
                    &player.sideboard,
                ),
            ] {
                for card in cards.iter().filter(|card| card.instance_id == id) {
                    if found.is_some() {
                        return Err(EngineError::new("ambiguous physical card movement source"));
                    }
                    found = Some((index, zone.clone(), card));
                }
            }
        }
        let Some((index, zone, card)) = found else {
            return Ok(None);
        };
        if constraint.is_some_and(|(name, player)| {
            *player != index || zone.as_ref().is_none_or(|zone| zone.name() != name)
        }) {
            return Ok(None);
        }
        let zone = zone.ok_or_else(|| EngineError::new("unknown card movement source zone"))?;
        if !self
            .state
            .players
            .iter()
            .any(|player| player.id == card.owner)
            || zone.owner().is_some_and(|owner| owner != card.owner)
        {
            return Err(EngineError::new("invalid card movement zone ownership"));
        }
        let identity = if matches!(zone, ObjectZone::OutsideGame(_)) {
            outside_game_movement_identity(card)?
        } else {
            object_identity_of(card)?
        };
        Ok(Some(CardMovementSource {
            zone,
            storage_player_id: self.state.players[index].id.clone(),
            identity,
        }))
    }

    /// Extraction is not itself a zone transition. Keep the old identity until
    /// the actual destination and replacement have been selected.
    pub(super) fn take_prepared_movement_card(
        &mut self,
        source: CardMovementSource,
    ) -> Result<OwnedMovementCard, EngineError> {
        let current = self
            .prepare_card_movement_source(&source.identity.instance_id, None)?
            .ok_or_else(|| EngineError::new("prepared card movement source disappeared"))?;
        if current.identity != source.identity
            || current.zone != source.zone
            || current.storage_player_id != source.storage_player_id
        {
            return Err(EngineError::new("prepared card movement source changed"));
        }
        let player = self.player_index(&source.storage_player_id)?;
        let cards = match &source.zone {
            ObjectZone::Library(_) => &mut self.state.players[player].library,
            ObjectZone::Hand(_) => &mut self.state.players[player].hand,
            ObjectZone::Graveyard(_) => &mut self.state.players[player].graveyard,
            ObjectZone::Exile => &mut self.state.players[player].exile,
            ObjectZone::Command => &mut self.state.players[player].command_zone,
            ObjectZone::OutsideGame(_) => &mut self.state.players[player].sideboard,
            ObjectZone::Battlefield | ObjectZone::Stack => {
                return Err(EngineError::new(
                    "card extraction requires its zone-specific lifecycle",
                ));
            }
        };
        let index = cards
            .iter()
            .position(|card| card.instance_id == source.identity.instance_id)
            .ok_or_else(|| EngineError::new("prepared movement carrier disappeared"))?;
        Ok(OwnedMovementCard {
            source,
            card: cards.remove(index),
        })
    }

    /// There is no clock reservation in OwnedMovementCard. Prepare and install
    /// synchronously at the real transition, after costs/replacements/callbacks.
    pub(super) fn commit_movement_identity(
        &mut self,
        moved: &mut OwnedMovementCard,
        destination: &ObjectZone,
        reason: ObjectTransitionReason,
    ) -> Result<(), EngineError> {
        self.commit_card_zone_transition(&mut moved.card, &moved.source, destination, reason)
    }

    pub(super) fn commit_card_zone_transition(
        &mut self,
        card: &mut CardInstance,
        source: &CardMovementSource,
        destination: &ObjectZone,
        reason: ObjectTransitionReason,
    ) -> Result<(), EngineError> {
        if matches!(source.zone, ObjectZone::OutsideGame(_)) {
            return self.admit_card_entering_the_game(card, source, destination, reason);
        }
        if object_identity_of(card)? != source.identity {
            return Err(EngineError::new(
                "movement carrier lost its authenticated source identity",
            ));
        }
        if destination.owner().is_some_and(|owner| owner != card.owner)
            || !self
                .state
                .players
                .iter()
                .any(|player| player.id == card.owner)
        {
            return Err(EngineError::new(
                "invalid card movement destination ownership",
            ));
        }
        let creates_object = match reason {
            ObjectTransitionReason::Move => source.zone != *destination,
            ObjectTransitionReason::Reorder => {
                if source.zone != *destination || !matches!(destination, ObjectZone::Library(_)) {
                    return Err(EngineError::new(
                        "reordering requires the same owner's library",
                    ));
                }
                false
            }
            ObjectTransitionReason::Reexile => {
                if !matches!(destination, ObjectZone::Exile) {
                    return Err(EngineError::new("re-exile requires the exile destination"));
                }
                true
            }
            ObjectTransitionReason::CommandNewObject => {
                if !matches!(destination, ObjectZone::Command) {
                    return Err(EngineError::new(
                        "command new-object instruction requires command",
                    ));
                }
                true
            }
        };
        if creates_object {
            // CR 400.7: the new object keeps no permission confined to the
            // zone it left (an impulse "you may play that card", an
            // Adventure's exile).
            self.end_card_permissions_confined_to(&card.instance_id, source.zone.name());
            let prepared = prepare_object_identities(
                self.state.battlefield_incarnation_sequence,
                &[&*card],
                IdentityAssignment::NewIncarnation,
            )?;
            install_object_identities(
                &mut self.state.battlefield_incarnation_sequence,
                &mut [card],
                prepared,
            )?;
        }
        Ok(())
    }
}

impl GameEngine {
    /// CR 400.11 and 400.7: a card brought into the game from outside it
    /// becomes a new object in its destination zone. It had no incarnation
    /// outside the game, so it is admitted rather than re-incarnated.
    fn admit_card_entering_the_game(
        &mut self,
        card: &mut CardInstance,
        source: &CardMovementSource,
        destination: &ObjectZone,
        reason: ObjectTransitionReason,
    ) -> Result<(), EngineError> {
        if outside_game_movement_identity(card)? != source.identity {
            return Err(EngineError::new(
                "outside-game carrier lost its authenticated source identity",
            ));
        }
        if !matches!(reason, ObjectTransitionReason::Move)
            || matches!(destination, ObjectZone::OutsideGame(_) | ObjectZone::Stack)
            || destination.owner().is_some_and(|owner| owner != card.owner)
            || source.zone.owner() != Some(card.owner.as_str())
        {
            return Err(EngineError::new(
                "invalid destination for a card entering the game",
            ));
        }
        self.admit_new_card_carrier(card)
    }
}

/// A card outside the game is authenticated by its carrier ID and owner only;
/// it must not already carry an incarnation (CR 400.11).
fn outside_game_movement_identity(card: &CardInstance) -> Result<ObjectIdentity, EngineError> {
    if existing_object_identity_of(card)?.is_some() {
        return Err(EngineError::new(
            "a card outside the game cannot carry a game-object incarnation",
        ));
    }
    Ok(ObjectIdentity {
        instance_id: card.instance_id.clone(),
        zone_identity: format!("outsideGame:{}:{}", card.owner, card.instance_id),
    })
}

/// Reset card characteristics while retaining the authenticated old incarnation
/// until a real destination commit installs the new one. Never admits a card.
pub(super) fn reset_owned_movement_card(moved: &mut OwnedMovementCard) -> Result<(), EngineError> {
    reset_movement_card(&mut moved.card, &moved.source)
}

pub(super) fn reset_movement_card(
    card: &mut CardInstance,
    source: &CardMovementSource,
) -> Result<(), EngineError> {
    if matches!(source.zone, ObjectZone::OutsideGame(_)) {
        // Nothing to reset: the card was never a game object, and its
        // admission belongs to the destination commit.
        return if outside_game_movement_identity(card)? == source.identity {
            Ok(())
        } else {
            Err(EngineError::new(
                "outside-game carrier lost its authenticated source identity",
            ))
        };
    }
    if object_identity_of(card)? != source.identity {
        return Err(EngineError::new(
            "departure reset requires the authenticated old incarnation",
        ));
    }
    super::reset_after_leaving_battlefield(card);
    GameEngine::assign_tracked_zone_identity(card, &source.identity.zone_identity);
    Ok(())
}

/// Destination replacement is selected once, before identity installation.
/// A one-shot replacement is authenticated by its exact unique value at commit,
/// not by a vector position that cleanup or another move could shift.
pub(super) enum PreparedGraveyardDestination {
    Graveyard {
        source_card_id: String,
    },
    Exile {
        source_card_id: String,
        one_shot_modifier: Option<serde_json::Value>,
        detail: serde_json::Value,
    },
}

impl PreparedGraveyardDestination {
    pub(super) fn source_card_id(&self) -> &str {
        match self {
            Self::Graveyard { source_card_id } | Self::Exile { source_card_id, .. } => {
                source_card_id
            }
        }
    }
}

impl GameEngine {
    pub(super) fn prepare_graveyard_destination(
        &self,
        player_index: usize,
        card: &CardInstance,
    ) -> PreparedGraveyardDestination {
        use super::*;
        let graveyard_owner_id = self.state.players[player_index].id.clone();
        if let Some(modifier_index) = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some("exileSpecificCardInsteadOfGraveyard")
                && modifier["cardInstanceId"].as_str() == Some(card.instance_id.as_str())
        }) {
            let modifier = &self.state.rule_modifiers[modifier_index];
            return PreparedGraveyardDestination::Exile {
                source_card_id: card.instance_id.clone(),
                one_shot_modifier: Some(modifier.clone()),
                detail: json!({"destination":"exile", "replacementSourceId":modifier["sourceCardInstanceId"]}),
            };
        }
        let replacement_source = self
            .battlefield_static_modifiers()
            .into_iter()
            .find(|(source, modifier)| {
                value_kind(modifier) == Some("replaceGraveyardDestination")
                    && matches_card_filter(&card.definition, &modifier["where"])
                    && self.event_player_reference_matches(
                        &modifier["graveyardOwners"],
                        source,
                        &graveyard_owner_id,
                    )
            })
            .map(|(source, _)| (source.instance_id, source.controller))
            .or_else(|| {
                self.state
                    .rule_modifiers
                    .iter()
                    .find(|modifier| {
                        value_kind(modifier) == Some("temporaryGraveyardDestinationReplacement")
                            && modifier["playerId"].as_str() == Some(graveyard_owner_id.as_str())
                            && modifier["expiresAfterTurn"].as_u64()
                                == Some(u64::from(self.state.turn_number))
                    })
                    .map(|modifier| {
                        (
                            modifier["sourceCardInstanceId"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            modifier["playerId"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                        )
                    })
            });
        match replacement_source {
            Some((source_id, source_controller)) => PreparedGraveyardDestination::Exile {
                source_card_id: card.instance_id.clone(),
                one_shot_modifier: None,
                detail: json!({"destination":"exile", "replacementSourceId":source_id, "replacementControllerId":source_controller}),
            },
            None => PreparedGraveyardDestination::Graveyard {
                source_card_id: card.instance_id.clone(),
            },
        }
    }

    pub(super) fn validate_prepared_graveyard_destination(
        &self,
        plan: &PreparedGraveyardDestination,
    ) -> Result<Option<usize>, EngineError> {
        let PreparedGraveyardDestination::Exile {
            one_shot_modifier: Some(expected),
            ..
        } = plan
        else {
            return Ok(None);
        };
        let mut matches = self
            .state
            .rule_modifiers
            .iter()
            .enumerate()
            .filter(|(_, current)| *current == expected)
            .map(|(index, _)| index);
        let first = matches.next().ok_or_else(|| {
            EngineError::new("prepared graveyard replacement is no longer available")
        })?;
        if matches.next().is_some() {
            return Err(EngineError::new(
                "prepared graveyard replacement is ambiguous",
            ));
        }
        Ok(Some(first))
    }

    pub(super) fn commit_graveyard_destination(
        &mut self,
        player_index: usize,
        card: CardInstance,
        destination: PreparedGraveyardDestination,
    ) -> Result<bool, EngineError> {
        if player_index >= self.state.players.len() {
            return Err(EngineError::new(
                "invalid prepared graveyard destination player",
            ));
        }
        if destination.source_card_id() != card.instance_id {
            return Err(EngineError::new(
                "prepared graveyard policy belongs to a different source card",
            ));
        }
        let consume = self.validate_prepared_graveyard_destination(&destination)?;
        match destination {
            PreparedGraveyardDestination::Graveyard { .. } => {
                self.state.players[player_index].graveyard.push(card);
                Ok(true)
            }
            PreparedGraveyardDestination::Exile { detail, .. } => {
                if let Some(index) = consume {
                    self.state.rule_modifiers.remove(index);
                }
                let card_id = card.instance_id.clone();
                let owner = self.state.players[player_index].id.clone();
                self.state.players[player_index].exile.push(card);
                self.record_event(
                    "graveyardDestinationReplaced",
                    Some(owner),
                    Some(card_id),
                    detail,
                );
                Ok(false)
            }
        }
    }
}
