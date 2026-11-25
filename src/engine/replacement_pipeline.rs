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

/// Adapter for an event family whose caller may not supply a chooser: a
/// deterministic order needs none, and a real CR 616.1 choice without one is
/// an explicit engine error instead of an arbitrary pick.
pub(super) struct RequiredReplacementChoiceProvider<'a>(pub Option<&'a mut dyn DecisionProvider>);
impl DecisionProvider for RequiredReplacementChoiceProvider<'_> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.0
            .as_mut()
            .ok_or_else(|| {
                EngineError::new(format!(
                    "replacement choice {} requires an explicit decision provider",
                    request.id
                ))
            })?
            .choose(state, request)
    }
}

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
    /// The affected card's own keyword replacement of one event, whose
    /// linked ability triggers when it applies (`exiled this way`): madness
    /// for a discard (CR 702.35a). `keyword` is recorded on the move.
    LinkedKeyword { keyword: &'static str },
}

/// Graveyard-destination replacement program (CR 614.1a "instead").
#[derive(Clone, Debug)]
pub(super) struct GraveyardReplacement {
    pub origin: GraveyardReplacementOrigin,
    /// Controller of the replacement effect, when it has one.
    pub controller: Option<String>,
    /// Where the object goes instead of its owner's graveyard.
    pub zone: ReplacedGraveyardZone,
    /// `When you do, ...` (CR 603.12): the reflexive triggered ability the
    /// applied replacement creates, with the object whose ability it is.
    pub reflexive: Option<(CardInstance, Value)>,
}

impl GraveyardReplacement {
    /// Whether applying this replacement does more than choose the zone: a
    /// reflexive ability or a linked keyword trigger depends on which
    /// replacement applied, so its owner's choice matters (CR 616.1).
    pub(super) fn has_consequence(&self) -> bool {
        self.reflexive.is_some()
            || matches!(
                self.origin,
                GraveyardReplacementOrigin::LinkedKeyword { .. }
            )
    }
}

/// The zone a graveyard-destination replacement sends the object to
/// instead (CR 614.1a): exile, its owner's hand, or its owner's library on
/// top, on the bottom, or shuffled in (CR 401.4, 701.24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReplacedGraveyardZone {
    Exile,
    Hand,
    LibraryTop,
    LibraryBottom,
    /// `[reveal it and] shuffle it into its owner's library`: a revealed
    /// object is shown to every player as it moves (CR 701.20a).
    LibraryShuffle {
        revealed: bool,
    },
}

impl ReplacedGraveyardZone {
    /// The typed replacement destination of a `to` IR value.
    pub(super) fn from_ir(to: &Value) -> Option<Self> {
        match value_kind(to)? {
            "exile" => Some(Self::Exile),
            "ownerHand" => Some(Self::Hand),
            "ownerLibrary" => match to["position"].as_str()? {
                "top" => Some(Self::LibraryTop),
                "bottom" => Some(Self::LibraryBottom),
                "shuffle" => Some(Self::LibraryShuffle {
                    revealed: match to.get("revealed") {
                        None => false,
                        Some(revealed) => revealed.as_bool()?,
                    },
                }),
                _ => None,
            },
            _ => None,
        }
    }

    /// The engine zone name of the destination, as departure records name it.
    pub(super) fn destination(self) -> &'static str {
        match self {
            Self::Exile => "exile",
            Self::Hand => "hand",
            Self::LibraryTop | Self::LibraryBottom | Self::LibraryShuffle { .. } => "library",
        }
    }

    /// The owner-relative zone the object enters (CR 400.3).
    pub(super) fn object_zone(self, owner: &str) -> super::card_zone_transition::ObjectZone {
        use super::card_zone_transition::ObjectZone;
        match self {
            Self::Exile => ObjectZone::Exile,
            Self::Hand => ObjectZone::Hand(owner.to_string()),
            Self::LibraryTop | Self::LibraryBottom | Self::LibraryShuffle { .. } => {
                ObjectZone::Library(owner.to_string())
            }
        }
    }

    /// The replacement detail recorded with `graveyardDestinationReplaced`.
    fn detail_position(self) -> Option<&'static str> {
        match self {
            Self::LibraryTop => Some("top"),
            Self::LibraryBottom => Some("bottom"),
            Self::LibraryShuffle { .. } => Some("shuffle"),
            Self::Exile | Self::Hand => None,
        }
    }
}

/// The static graveyard-destination modifier shape (CR 614.1a). `from`
/// distinguishes dying (CR 700.4: from the battlefield) from a move to a
/// graveyard from any zone; the moving object is matched by its printed
/// card filter (`where`), its effective permanent filter (`permanentWhere`,
/// only from the battlefield), its controller and owner relations, and an
/// optional restriction to the source itself.
pub(super) fn graveyard_destination_modifier_supported(modifier: &Value) -> bool {
    let from = value_kind(&modifier["from"]);
    let battlefield = from == Some("battlefield");
    player_reference_supported(&modifier["graveyardOwners"])
        && modifier.get("where").is_none_or(card_filter_supported)
        && matches!(from, Some("anyZone" | "battlefield"))
        && ReplacedGraveyardZone::from_ir(&modifier["to"]).is_some()
        && modifier
            .get("permanentWhere")
            .is_none_or(|filter| battlefield && permanent_filter_supported(filter))
        && modifier
            .get("controller")
            .is_none_or(|player| battlefield && event_player_pattern_supported(player))
        && modifier
            .get("owner")
            .is_none_or(event_player_pattern_supported)
        && modifier
            .get("object")
            .is_none_or(|object| value_kind(object) == Some("self"))
        && modifier.get("excludeSource").is_none_or(Value::is_boolean)
        && modifier
            .get("damagedBySourceThisTurn")
            .is_none_or(|damaged| battlefield && damaged.is_boolean())
        && modifier
            .get("onReplaced")
            .is_none_or(triggered_rule_supported)
}

/// The player relations `event_player_reference_matches` evaluates.
fn event_player_pattern_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "eachPlayer") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("opponentsOf") => event_player_pattern_supported(&reference["player"]),
        _ => false,
    }
}

/// The temporary graveyard-destination replacement an effect creates
/// (CR 611.2a, 614.1a): the same modifier shape, installed with its source,
/// controller and lifetime, optionally restricted to the objects an earlier
/// action of the creating effect affected (`a creature dealt damage this
/// way`: the permanents the resolving object dealt damage to).
pub(super) fn temporary_graveyard_destination_supported(effect: &Value) -> bool {
    graveyard_destination_modifier_supported(&effect["replacement"])
        && value_kind(&effect["replacement"]["object"]).is_none()
        && matches!(
            value_kind(&effect["duration"]),
            Some("untilEndOfCurrentTurn")
        )
        && effect
            .get("objects")
            .is_none_or(|objects| value_kind(objects) == Some("damagedThisResolution"))
}

impl GameEngine {
    /// Propose every replacement of `card`'s move to `graveyard_owner_id`'s
    /// graveyard, in the precedence order callers rely on: the finality
    /// counter, card-specific one-shot modifiers, the card's own
    /// self-replacement while it is not on the battlefield, static modifiers,
    /// then temporary ones.
    pub(super) fn propose_graveyard_destination_replacements(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        static_modifiers: &[(CardInstance, Value)],
    ) -> Vec<ReplacementProposal<GraveyardReplacement>> {
        let mut proposals = Vec::new();
        let on_battlefield = self.permanent_position(&card.instance_id).is_some();
        // The finality counter (CR 122.1i): a permanent with one on it that
        // would be put into a graveyard from the battlefield is exiled
        // instead. The counter is read while the permanent is still there,
        // by every departure path (single moves and destruction batches).
        if card.counters.get("finality").copied().unwrap_or(0) > 0 && on_battlefield {
            proposals.push(ReplacementProposal {
                id: format!("{}:graveyardFinalityCounter", card.instance_id),
                source_id: card.instance_id.clone(),
                source_name: card.definition.name.clone(),
                program: GraveyardReplacement {
                    origin: GraveyardReplacementOrigin::Static,
                    controller: Some(card.controller.clone()),
                    zone: ReplacedGraveyardZone::Exile,
                    reflexive: None,
                },
            });
        }
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
                        zone: ReplacedGraveyardZone::Exile,
                        reflexive: None,
                    },
                });
            }
        }
        // An object's own `if THIS would be put into a graveyard from
        // anywhere` replacement functions wherever the object is (CR 113.6):
        // off the battlefield it is read from the card itself; on the
        // battlefield it is one of the static modifiers below.
        if !on_battlefield {
            let own = card
                .definition
                .printed_rules()
                .iter()
                .filter(|rule| value_kind(rule) == Some("staticAbility"))
                .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
                .filter(|modifier| {
                    value_kind(modifier) == Some("replaceGraveyardDestination")
                        && value_kind(&modifier["object"]) == Some("self")
                        && value_kind(&modifier["from"]) == Some("anyZone")
                        && graveyard_destination_modifier_supported(modifier)
                })
                .cloned()
                .collect::<Vec<_>>();
            for (ordinal, modifier) in own.iter().enumerate() {
                if let Some(program) =
                    self.static_graveyard_replacement(card, graveyard_owner_id, card, modifier)
                {
                    proposals.push(ReplacementProposal {
                        id: format!("{}:graveyardSelf:{ordinal}", card.instance_id),
                        source_id: card.instance_id.clone(),
                        source_name: card.definition.name.clone(),
                        program,
                    });
                }
            }
        }
        for (ordinal, (source, modifier)) in static_modifiers.iter().enumerate() {
            if value_kind(modifier) != Some("replaceGraveyardDestination") {
                continue;
            }
            if let Some(program) =
                self.static_graveyard_replacement(card, graveyard_owner_id, source, modifier)
            {
                proposals.push(ReplacementProposal {
                    id: format!("{}:graveyardStatic:{ordinal}", source.instance_id),
                    source_id: source.instance_id.clone(),
                    source_name: source.definition.name.clone(),
                    program,
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
                        zone: ReplacedGraveyardZone::Exile,
                        reflexive: None,
                    },
                });
            }
        }
        for (ordinal, installed) in self.state.rule_modifiers.iter().enumerate() {
            if let Some(program) =
                self.created_graveyard_replacement(card, graveyard_owner_id, installed)
            {
                let source_id = installed["sourceCardInstanceId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                proposals.push(ReplacementProposal {
                    id: format!("{source_id}:graveyardCreated:{ordinal}"),
                    source_name: self.card_name_for_instance(&source_id),
                    source_id,
                    program,
                });
            }
        }
        proposals
    }

    /// The program of one static `replaceGraveyardDestination` modifier of
    /// `source` for `card`'s move to `graveyard_owner_id`'s graveyard, or
    /// `None` when its event pattern does not match (CR 614.1a, 700.4).
    fn static_graveyard_replacement(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        source: &CardInstance,
        modifier: &Value,
    ) -> Option<GraveyardReplacement> {
        if !self.graveyard_replacement_pattern_matches(card, graveyard_owner_id, source, modifier) {
            return None;
        }
        Some(GraveyardReplacement {
            origin: GraveyardReplacementOrigin::Static,
            controller: Some(source.controller.clone()),
            zone: ReplacedGraveyardZone::from_ir(&modifier["to"])?,
            reflexive: modifier
                .get("onReplaced")
                .map(|ability| (source.clone(), ability.clone())),
        })
    }

    /// One effect-created graveyard-destination replacement (CR 611.2a):
    /// it lasts through this turn and acts for its controller, whose
    /// relations (`you control`, `an opponent controls`) it reads even if
    /// its source has left the battlefield since (CR 611.2c).
    fn created_graveyard_replacement(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        installed: &Value,
    ) -> Option<GraveyardReplacement> {
        if value_kind(installed) != Some("createdGraveyardDestinationReplacement")
            || installed["expiresAfterTurn"].as_u64() != Some(u64::from(self.state.turn_number))
        {
            return None;
        }
        // `a creature dealt damage this way`: only the recorded objects.
        if let Some(objects) = installed.get("objectIds").and_then(Value::as_array)
            && !objects
                .iter()
                .any(|id| id.as_str() == Some(card.instance_id.as_str()))
        {
            return None;
        }
        let controller = installed["controllerId"].as_str()?.to_string();
        let mut perspective = card.clone();
        perspective.controller = controller.clone();
        perspective.instance_id = installed["sourceCardInstanceId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let modifier = &installed["replacement"];
        if !self.graveyard_replacement_pattern_matches(
            card,
            graveyard_owner_id,
            &perspective,
            modifier,
        ) {
            return None;
        }
        Some(GraveyardReplacement {
            origin: GraveyardReplacementOrigin::Temporary,
            controller: Some(controller),
            zone: ReplacedGraveyardZone::from_ir(&modifier["to"])?,
            reflexive: None,
        })
    }

    /// The event pattern of a graveyard-destination modifier of `source`:
    /// origin zone, graveyard owner, the moving object's printed and
    /// effective characteristics, and its player relations.
    fn graveyard_replacement_pattern_matches(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        source: &CardInstance,
        modifier: &Value,
    ) -> bool {
        let on_battlefield = self.permanent_position(&card.instance_id).is_some();
        let from = match value_kind(&modifier["from"]) {
            Some("anyZone") => true,
            // Dying (CR 700.4): put into a graveyard from the battlefield.
            Some("battlefield") => on_battlefield,
            _ => false,
        };
        from && (value_kind(&modifier["object"]) != Some("self")
            || card.instance_id == source.instance_id)
            && !(modifier["excludeSource"].as_bool() == Some(true)
                && card.instance_id == source.instance_id)
            && matches_card_filter(&card.definition, &modifier["where"])
            && (modifier["damagedBySourceThisTurn"].as_bool() != Some(true)
                || self.permanent_dealt_damage_by_this_turn(card, &source.instance_id))
            && modifier.get("permanentWhere").is_none_or(|filter| {
                on_battlefield && self.permanent_matches_effective_filter(card, filter)
            })
            && modifier.get("controller").is_none_or(|player| {
                on_battlefield
                    && self.event_player_reference_matches(player, source, &card.controller)
            })
            && modifier.get("owner").is_none_or(|player| {
                self.event_player_reference_matches(player, source, &card.owner)
            })
            && self.event_player_reference_matches(
                &modifier["graveyardOwners"],
                source,
                graveyard_owner_id,
            )
    }

    /// The one graveyard-destination replacement that applies to `card`.
    /// Once one applied, the event no longer puts the card into a graveyard,
    /// so no other proposal matches it (CR 614.5) and at most one proposal is
    /// returned. When the applicable proposals lead to different events, the
    /// controller of the affected permanent, or the owner of the affected
    /// card outside the battlefield, chooses (CR 616.1); without a decision
    /// provider that choice is an explicit engine error, never a pick.
    pub(super) fn select_graveyard_destination_replacement(
        &self,
        card: &CardInstance,
        graveyard_owner_id: &str,
        static_modifiers: &[(CardInstance, Value)],
        provider: Option<&mut dyn DecisionProvider>,
    ) -> Result<Option<ReplacementProposal<GraveyardReplacement>>, EngineError> {
        let proposals = self.propose_graveyard_destination_replacements(
            card,
            graveyard_owner_id,
            static_modifiers,
        );
        self.select_graveyard_destination_proposal(card, &proposals, provider)
    }

    /// Orders and applies one of `proposals` (the event-specific ones the
    /// caller added first, such as a discard's madness exile) for `card`.
    pub(super) fn select_graveyard_destination_proposal(
        &self,
        card: &CardInstance,
        proposals: &[ReplacementProposal<GraveyardReplacement>],
        provider: Option<&mut dyn DecisionProvider>,
    ) -> Result<Option<ReplacementProposal<GraveyardReplacement>>, EngineError> {
        // Proposals with one outcome (the same zone and no reflexive or
        // linked ability) produce the same event and differ only in
        // attribution: the order chosen cannot change the event (CR 616.1),
        // so the first in precedence applies (a card-specific one-shot
        // modifier first).
        let equivalent = proposals
            .iter()
            .all(|proposal| !proposal.program.has_consequence())
            && proposals
                .windows(2)
                .all(|pair| pair[0].program.zone == pair[1].program.zone);
        let chooser = if self.permanent_position(&card.instance_id).is_some() {
            card.controller.clone()
        } else {
            card.owner.clone()
        };
        let choice = ReplacementOrder::AffectedChooses {
            player_id: &chooser,
            request_prefix: "graveyard-destination-replacement",
            decision_key: "graveyardReplacementId",
        };
        let order = if equivalent {
            &ReplacementOrder::FirstProposed
        } else {
            &choice
        };
        // The event: whether the card still goes to a graveyard.
        let mut to_graveyard = true;
        let mut selected = None;
        let mut provider = RequiredReplacementChoiceProvider(provider);
        self.run_replacement_pipeline(
            proposals,
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

    /// Puts a card whose graveyard destination was replaced into its new
    /// zone of `owner_index` (CR 614.1a, 400.3): the top of a library is
    /// its last element, the bottom its first; a shuffle randomizes the
    /// whole library after the card joins it (CR 701.24a).
    pub(super) fn place_replaced_graveyard_card(
        &mut self,
        owner_index: usize,
        card: CardInstance,
        zone: ReplacedGraveyardZone,
    ) {
        use rand::seq::SliceRandom;
        let player = &mut self.state.players[owner_index];
        match zone {
            ReplacedGraveyardZone::Exile => player.exile.push(card),
            ReplacedGraveyardZone::Hand => player.hand.push(card),
            ReplacedGraveyardZone::LibraryTop => player.library.push(card),
            ReplacedGraveyardZone::LibraryBottom => player.library.insert(0, card),
            ReplacedGraveyardZone::LibraryShuffle { .. } => {
                player.library.push(card);
                player.library.shuffle(&mut self.rng);
            }
        }
    }

    /// `When you do, ...` (CR 603.12): the reflexive triggered ability an
    /// applied replacement creates triggers once the replacement applied.
    pub(super) fn publish_graveyard_replacement_reflexive(
        &mut self,
        reflexive: Option<(CardInstance, Value)>,
    ) {
        if let Some((source, ability)) = reflexive {
            self.put_triggered_ability_on_stack_with_origin(
                &source,
                ability,
                BTreeMap::new(),
                super::trigger_multiplicity::TriggerOrigin::Unproven,
            );
        }
    }

    /// Whether `card` was dealt damage by `source_id` this turn (CR 120.1),
    /// from the turn's damage records.
    fn permanent_dealt_damage_by_this_turn(&self, card: &CardInstance, source_id: &str) -> bool {
        self.state.events.iter().any(|event| {
            event.turn_number == self.state.turn_number
                && event.kind == "damageDealt"
                && event.card_instance_id.as_deref() == Some(card.instance_id.as_str())
                && event.detail["source"].as_str() == Some(source_id)
                && event.detail["amount"]
                    .as_i64()
                    .is_some_and(|amount| amount > 0)
        })
    }

    /// The permanents the resolving object dealt damage to while it
    /// resolves (`dealt damage this way`, CR 608.2c): the damage records of
    /// its source after the cast that put it on the stack, or this turn's
    /// for an object no cast record names (a copy).
    fn permanents_damaged_by_resolving_object(&self, stack_object: &StackObject) -> Vec<String> {
        let source_id = stack_object.card.instance_id.as_str();
        let since = self
            .state
            .events
            .iter()
            .rev()
            .find(|event| {
                event.kind == "spellCast"
                    && event.detail["stackId"].as_str() == Some(stack_object.id.as_str())
            })
            .map(|event| event.sequence);
        let mut ids = Vec::<String>::new();
        for event in &self.state.events {
            if event.turn_number == self.state.turn_number
                && since.is_none_or(|since| event.sequence > since)
                && event.kind == "damageDealt"
                && event.detail["source"].as_str() == Some(source_id)
                && event.detail["amount"]
                    .as_i64()
                    .is_some_and(|amount| amount > 0)
                && let Some(id) = &event.card_instance_id
                && !ids.contains(id)
            {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// Installs an effect-created graveyard-destination replacement
    /// (CR 611.2a, 614.1a) for the controller of the resolving object.
    /// `objects: damagedThisResolution` restricts it to the permanents the
    /// resolving object dealt damage to (`a creature dealt damage this way`).
    pub(super) fn install_created_graveyard_destination_replacement(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
    ) {
        let mut installed = json!({
            "kind": "createdGraveyardDestinationReplacement",
            "replacement": effect["replacement"].clone(),
            "sourceCardInstanceId": stack_object.card.instance_id.clone(),
            "controllerId": stack_object.controller.clone(),
            "expiresAfterTurn": self.state.turn_number,
        });
        if value_kind(&effect["objects"]) == Some("damagedThisResolution") {
            installed["objectIds"] =
                json!(self.permanents_damaged_by_resolving_object(stack_object));
        }
        self.state.rule_modifiers.push(installed);
    }
}

/// The `graveyardDestinationReplaced` detail of a replaced move.
pub(super) fn graveyard_replacement_detail(
    source_id: &str,
    controller_id: Option<&str>,
    zone: ReplacedGraveyardZone,
) -> Value {
    let mut detail = json!({
        "destination": zone.destination(),
        "replacementSourceId": source_id,
    });
    if let Some(position) = zone.detail_position() {
        detail["position"] = json!(position);
    }
    if zone == (ReplacedGraveyardZone::LibraryShuffle { revealed: true }) {
        detail["revealed"] = json!(true);
    }
    if let Some(controller_id) = controller_id {
        detail["replacementControllerId"] = json!(controller_id);
    }
    detail
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "replacement_pipeline_tests.rs"]
mod tests;
