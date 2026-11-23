//! Replacement pipeline (CR 614, 616): propose -> match -> order -> apply.
//!
//! An event family proposes its candidate replacement effects once, from
//! static modifiers and installed rule modifiers. The pipeline then repeats:
//! match the proposals that still apply to the (possibly already modified)
//! event, order them by the family's policy, and apply exactly one. Each
//! proposal applies at most once to one event (CR 616.1e, 614.5). Families
//! own only their proposal source, their match predicate and their apply
//! step; the loop, the ordering decision and the applied-identity record are
//! shared here.

use super::*;

/// One replacement effect proposed for one event, with a stable identity.
#[derive(Clone, Debug)]
pub(super) struct ReplacementProposal<P> {
    /// Stable identity, distinct for separate identical modifiers.
    pub id: String,
    /// Source object of the replacement effect, for attribution and labels.
    pub source_id: String,
    pub source_name: String,
    pub program: P,
}

/// How applicable replacements are ordered for one event.
pub(super) enum ReplacementOrder<'a> {
    /// CR 616.1: the affected player, or the controller of the affected
    /// object, chooses one applicable replacement at a time.
    AffectedChooses {
        player_id: &'a str,
        request_prefix: &'a str,
        decision_key: &'a str,
    },
    /// The event family admits one applicable replacement; several are an
    /// explicit engine error instead of an arbitrary pick.
    RequireUnique { ambiguity: String },
    /// Every proposal produces the same outcome and differs only in
    /// attribution: the first proposal in proposal order applies.
    FirstProposed,
}

impl GameEngine {
    /// Run the match -> order -> apply loop over `proposals` until no unused
    /// proposal matches `event`. Returns the applied identities in order.
    pub(super) fn run_replacement_pipeline<P, E, D: DecisionProvider>(
        &self,
        proposals: &[ReplacementProposal<P>],
        event: &mut E,
        order: &ReplacementOrder<'_>,
        provider: &mut D,
        matches: impl Fn(&ReplacementProposal<P>, &E) -> bool,
        mut apply: impl FnMut(&ReplacementProposal<P>, &mut E) -> Result<(), EngineError>,
    ) -> Result<Vec<String>, EngineError> {
        let mut used = BTreeSet::new();
        let mut applied = Vec::new();
        loop {
            let applicable = proposals
                .iter()
                .enumerate()
                .filter(|(index, proposal)| !used.contains(index) && matches(proposal, event))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if applicable.is_empty() {
                break;
            }
            let index =
                self.order_replacements(proposals, &applicable, order, applied.len(), provider)?;
            apply(&proposals[index], event)?;
            used.insert(index);
            applied.push(proposals[index].id.clone());
        }
        Ok(applied)
    }

    fn order_replacements<P, D: DecisionProvider>(
        &self,
        proposals: &[ReplacementProposal<P>],
        applicable: &[usize],
        order: &ReplacementOrder<'_>,
        step: usize,
        provider: &mut D,
    ) -> Result<usize, EngineError> {
        if applicable.len() == 1 {
            return Ok(applicable[0]);
        }
        match order {
            ReplacementOrder::FirstProposed => Ok(applicable[0]),
            ReplacementOrder::RequireUnique { ambiguity } => {
                Err(EngineError::new(ambiguity.clone()))
            }
            ReplacementOrder::AffectedChooses {
                player_id,
                request_prefix,
                decision_key,
            } => {
                let options = applicable
                    .iter()
                    .map(|index| {
                        let proposal = &proposals[*index];
                        ReplacementChoiceOption {
                            id: &proposal.id,
                            source_id: &proposal.source_id,
                            source_name: &proposal.source_name,
                        }
                    })
                    .collect::<Vec<_>>();
                let chosen = self.choose_replacement(
                    player_id,
                    request_prefix,
                    decision_key,
                    step,
                    &options,
                    provider,
                )?;
                applicable
                    .iter()
                    .copied()
                    .find(|index| proposals[*index].id == chosen)
                    .ok_or_else(|| EngineError::new("replacement choice has no matching proposal"))
            }
        }
    }

    /// CR 616.1: ask the affected player which applicable replacement applies
    /// next. Families with their own applicability rules (damage) reuse this
    /// request instead of building another one. Returns the chosen id.
    pub(super) fn choose_replacement<D: DecisionProvider>(
        &self,
        player_id: &str,
        request_prefix: &str,
        decision_key: &str,
        step: usize,
        options: &[ReplacementChoiceOption<'_>],
        provider: &mut D,
    ) -> Result<String, EngineError> {
        let request = EngineDecisionRequest {
            id: format!(
                "{request_prefix}:{}:{}:{step}",
                self.state.turn_number,
                self.state.events.len(),
            ),
            kind: DecisionKind::ReplacementChoice,
            player_id: player_id.to_string(),
            source_card: None,
            source_card_instance_id: None,
            choice: None,
            options: options
                .iter()
                .map(|option| LegalAction {
                    id: option.id.to_string(),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.to_string(),
                    label: format!("Apply {}", option.source_name),
                    card_instance_id: Some(option.source_id.to_string()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(decision_key.to_string(), json!(option.id))]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                })
                .collect(),
        };
        Ok(self.choose_action(provider, request)?.id)
    }
}

/// One option of a replacement-order decision.
pub(super) struct ReplacementChoiceOption<'a> {
    pub id: &'a str,
    pub source_id: &'a str,
    pub source_name: &'a str,
}

/// Origin of a proposed graveyard-destination replacement.
#[derive(Clone, Debug)]
pub(super) enum GraveyardReplacementOrigin {
    /// An installed one-shot modifier that names this card; it is consumed.
    SpecificCard { modifier: Value },
    /// A static `replaceGraveyardDestination` modifier of a permanent.
    Static,
    /// An installed "this turn" replacement for one graveyard owner.
    Temporary,
}

/// Graveyard-destination replacement program (CR 614.1a "instead").
#[derive(Clone, Debug)]
pub(super) struct GraveyardReplacement {
    pub origin: GraveyardReplacementOrigin,
    /// Controller of the replacement effect, when it has one.
    pub controller: Option<String>,
}

impl GameEngine {
    /// Propose every replacement that would send `card` to exile instead of
    /// its owner's graveyard, in the precedence order that callers rely on:
    /// card-specific one-shot modifiers, static modifiers, temporary ones.
    pub(super) fn propose_graveyard_destination_replacements(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        static_modifiers: &[(CardInstance, Value)],
    ) -> Vec<ReplacementProposal<GraveyardReplacement>> {
        let mut proposals = Vec::new();
        for (ordinal, modifier) in self.state.rule_modifiers.iter().enumerate() {
            if value_kind(modifier) == Some("exileSpecificCardInsteadOfGraveyard")
                && modifier["cardInstanceId"].as_str() == Some(card.instance_id.as_str())
            {
                let source_id = modifier["sourceCardInstanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                proposals.push(ReplacementProposal {
                    id: format!("{source_id}:graveyardSpecific:{ordinal}"),
                    source_name: self.card_name_for_instance(&source_id),
                    source_id,
                    program: GraveyardReplacement {
                        origin: GraveyardReplacementOrigin::SpecificCard {
                            modifier: modifier.clone(),
                        },
                        controller: None,
                    },
                });
            }
        }
        for (ordinal, (source, modifier)) in static_modifiers.iter().enumerate() {
            if value_kind(modifier) == Some("replaceGraveyardDestination")
                && matches_card_filter(&card.definition, &modifier["where"])
                && self.event_player_reference_matches(
                    &modifier["graveyardOwners"],
                    source,
                    graveyard_owner_id,
                )
            {
                proposals.push(ReplacementProposal {
                    id: format!("{}:graveyardStatic:{ordinal}", source.instance_id),
                    source_id: source.instance_id.clone(),
                    source_name: source.definition.name.clone(),
                    program: GraveyardReplacement {
                        origin: GraveyardReplacementOrigin::Static,
                        controller: Some(source.controller.clone()),
                    },
                });
            }
        }
        for (ordinal, modifier) in self.state.rule_modifiers.iter().enumerate() {
            if value_kind(modifier) == Some("temporaryGraveyardDestinationReplacement")
                && modifier["playerId"].as_str() == Some(graveyard_owner_id)
                && modifier["expiresAfterTurn"].as_u64() == Some(u64::from(self.state.turn_number))
            {
                let source_id = modifier["sourceCardInstanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                proposals.push(ReplacementProposal {
                    id: format!("{source_id}:graveyardTemporary:{ordinal}"),
                    source_name: self.card_name_for_instance(&source_id),
                    source_id,
                    program: GraveyardReplacement {
                        origin: GraveyardReplacementOrigin::Temporary,
                        controller: modifier["playerId"].as_str().map(ToOwned::to_owned),
                    },
                });
            }
        }
        proposals
    }

    /// The one graveyard-destination replacement that applies to `card`
    /// under `order`. Every proposal sends the card to exile, so the event
    /// stops matching once one applied (its destination is no longer a
    /// graveyard) and at most one proposal is returned.
    pub(super) fn select_graveyard_destination_replacement(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        static_modifiers: &[(CardInstance, Value)],
        order: &ReplacementOrder<'_>,
    ) -> Result<Option<ReplacementProposal<GraveyardReplacement>>, EngineError> {
        let proposals = self.propose_graveyard_destination_replacements(
            card,
            graveyard_owner_id,
            static_modifiers,
        );
        // The event: whether the card still goes to a graveyard.
        let mut to_graveyard = true;
        let mut selected = None;
        let mut provider = super::quantity_replacement::OptionalQuantityProvider(None);
        self.run_replacement_pipeline(
            &proposals,
            &mut to_graveyard,
            order,
            &mut provider,
            |_, to_graveyard| *to_graveyard,
            |proposal, to_graveyard| {
                *to_graveyard = false;
                selected = Some(proposal.clone());
                Ok(())
            },
        )?;
        Ok(selected)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "replacement_pipeline_tests.rs"]
mod tests;
