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
//! - `removeCounters`: at least the stated number of counters (one at
//!   least) left the one named object (CR 122.8).
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
//! - `dealDamage`: some damage was actually dealt (CR 120.1); prevented
//!   damage is not dealt (CR 615.1).
//! - `loseLife`: the player lost at least the stated amount (CR 119.3).
//! - `gainControlPermanent`: every referenced permanent is controlled by the
//!   new controller afterwards (CR 108.4, 613.1b).
//! - `attachPermanent`: every attachment became attached to the recipient;
//!   one already attached to it doesn't move (CR 701.3b).
//! - `modifyPowerToughness` / `grantKeyword` on one object: the object was a
//!   permanent the effect applied to (CR 611.2c, 400.7).
//! - `payCost`: the payer paid the whole resolution cost (CR 118.3, 608.2c).
//! - `chooseOneEffect` (`sacrifice an artifact or discard a card`): the
//!   option the performer chose was performed, as its own instruction group.
//!
//! An instruction group (`actions`: `reveal a Dinosaur card from your hand`,
//! `exile two cards from your graveyard`, `discard your hand and draw two
//! cards`) is performed when every measured member was performed and every
//! choice it makes chose the number the instruction asks for. A choice whose
//! count is clamped to the available population (`minimum(N, count(...))`,
//! the CR 609.3 "as much as possible" reading of a mandatory instruction)
//! still asks for N: choosing fewer did not perform the instruction. A member
//! that reads a result of an earlier member (a chosen card, a bound `X`) is
//! measured after the group, against the zones its objects occupied before.
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
        Some("removeCounters") => super::quantity_cost_payers::counter_removal_supported(action),
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
        Some("dealDamage") => action["recipient"].is_object(),
        Some("loseLife") => single_player(&action["player"]),
        Some("gainControlPermanent") => {
            action["permanent"].is_object() && player_reference_supported(&action["controller"])
        }
        Some("attachPermanent") => action["attachment"].is_object() && action["to"].is_object(),
        Some("modifyPowerToughness" | "grantKeyword") => action["object"].is_object(),
        // The payer actually paid the resolution cost (`payments.rs`).
        Some("payCost") => single_player(&action["player"]),
        _ => false,
    }
}

/// `chooseOneEffect` as a measured instruction (`sacrifice an artifact or
/// discard a card`): the performer chooses one option, and each option is
/// its own measured instruction group.
fn measured_disjunction_supported(action: &Value) -> bool {
    value_kind(action) == Some("chooseOneEffect")
        && action["options"].as_array().is_some_and(|options| {
            options.len() >= 2
                && options
                    .iter()
                    .all(|option| measured_group_supported(&option["effects"]))
        })
}

/// Static contract of the measured instruction of `optionalAction` and
/// `performAction`: exactly one of a supported primitive (`action`) or a
/// supported instruction group (`actions`).
pub(super) fn measured_instruction_supported(effect: &Value) -> bool {
    match (effect.get("action"), effect.get("actions")) {
        (Some(action), None) => {
            effect_supported(action)
                && (optional_action_witness_supported(action)
                    || measured_disjunction_supported(action))
        }
        (None, Some(actions)) => measured_group_supported(actions),
        _ => false,
    }
}

/// `performAction{action|actions, onPerformed, onDeclined}`: a mandatory
/// instruction whose measured performance selects the outcome (`If you do`,
/// `When you do`, `If you can't` / `If you don't`). At least one outcome
/// program is nonempty.
pub(super) fn performed_action_supported(effect: &Value) -> bool {
    let outcome = |key: &str| {
        effect.get(key).is_none_or(|effects| {
            effects
                .as_array()
                .is_some_and(|effects| effects.iter().all(effect_supported))
        })
    };
    let nonempty = |key: &str| {
        effect
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|effects| !effects.is_empty())
    };
    measured_instruction_supported(effect)
        && outcome("onPerformed")
        && outcome("onDeclined")
        && (nonempty("onPerformed") || nonempty("onDeclined"))
}

/// The role one member plays in an instruction group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupRole {
    /// A choice whose chosen objects a later member acts on; it must choose
    /// the number the instruction asks for.
    Selection,
    /// A binding or an always-performed bookkeeping step (shuffle, search
    /// record): it never decides performance.
    Neutral,
    /// An action whose performance is measured.
    Measured,
}

fn group_role(member: &Value) -> Option<GroupRole> {
    match value_kind(member) {
        Some("chooseCards" | "choosePermanents") if member["id"].is_string() => {
            Some(GroupRole::Selection)
        }
        Some("bind") if member["id"].is_string() => Some(GroupRole::Neutral),
        Some("shuffleZone" | "shuffleLibrariesHolding" | "recordLibrarySearch") => {
            Some(GroupRole::Neutral)
        }
        Some("revealCards") => member["cards"].is_object().then_some(GroupRole::Measured),
        Some("discardCards") if member.get("cards").is_some() => {
            member["cards"].is_object().then_some(GroupRole::Measured)
        }
        Some("sacrificePermanents") if member.get("objects").is_some() => {
            member["objects"].is_object().then_some(GroupRole::Measured)
        }
        _ => optional_action_witness_supported(member).then_some(GroupRole::Measured),
    }
}

/// Members measured only inside a group: their objects (`cards`,
/// `objects`) are measured after the group, against the zones they occupied
/// before it, never by a primitive's count witness.
fn group_measured_member(member: &Value) -> bool {
    match value_kind(member) {
        Some("revealCards") => true,
        Some("discardCards") => member.get("cards").is_some(),
        Some("sacrificePermanents") => member.get("objects").is_some(),
        _ => false,
    }
}

/// Static contract of an instruction group: every member is a supported
/// effect with a role, and at least one member is measured.
pub(super) fn measured_group_supported(actions: &Value) -> bool {
    let Some(members) = actions.as_array() else {
        return false;
    };
    let mut measured = false;
    for member in members {
        if !effect_supported(member) {
            return false;
        }
        match group_role(member) {
            Some(GroupRole::Measured) => measured = true,
            Some(GroupRole::Selection | GroupRole::Neutral) => {}
            None => return false,
        }
    }
    measured
}

/// One member's measurement, prepared before the group runs.
enum MemberMeasure {
    Neutral,
    /// The choice `id` must have chosen the requested number of objects.
    Selection(Value),
    /// A member whose operands were resolvable before the group ran.
    Prepared(PerformanceWitness),
    /// A member reading an earlier member's result, measured afterwards.
    Deferred(Value),
}

/// A group's measurement prepared before it runs.
struct GroupMeasure {
    first_event: usize,
    zones: BTreeMap<String, &'static str>,
    members: Vec<MemberMeasure>,
}

/// Result names (`id`, `bind`) a member produces for later members.
fn produced_names(member: &Value, names: &mut BTreeSet<String>) {
    for key in ["id", "bind"] {
        if value_kind(member) != Some("chooseTargets")
            && let Some(name) = member.get(key).and_then(Value::as_str)
        {
            names.insert(name.to_string());
        }
    }
}

/// True when `value` reads one of `names` as a decision or a binding.
fn reads_any(value: &Value, names: &BTreeSet<String>) -> bool {
    match value {
        Value::Object(map) => {
            ["decisionId", "binding"].iter().any(|key| {
                map.get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|name| names.contains(name))
            }) || (value_kind(value) == Some("boundValue")
                && map
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|name| names.contains(name)))
                || map.values().any(|child| reads_any(child, names))
        }
        Value::Array(items) => items.iter().any(|item| reads_any(item, names)),
        _ => false,
    }
}

impl GameEngine {
    /// Runs the measured instruction of `optionalAction` / `performAction`
    /// and returns whether it was actually performed (CR 608.2c, 603.12).
    pub(super) fn run_measured_instruction<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        if let Some(actions) = effect.get("actions") {
            let measure = self.prepare_group_measure(actions, stack_object, bindings, decisions);
            self.execute_effects(actions, stack_object, bindings, decisions, provider)?;
            return Ok(self.group_was_performed(&measure, stack_object, bindings, decisions));
        }
        let action = &effect["action"];
        if value_kind(action) == Some("chooseOneEffect") {
            // Only the chosen option runs; the others never perform.
            let (read_bindings, read_decisions) = (&*bindings, &*decisions);
            let options = action["options"]
                .as_array()
                .map(|options| {
                    options
                        .iter()
                        .map(|option| {
                            self.prepare_group_measure(
                                &option["effects"],
                                stack_object,
                                read_bindings,
                                read_decisions,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.execute_effects(
                &Value::Array(vec![action.clone()]),
                stack_object,
                bindings,
                decisions,
                provider,
            )?;
            let (read_bindings, read_decisions) = (&*bindings, &*decisions);
            return Ok(options.iter().any(|measure| {
                self.group_was_performed(measure, stack_object, read_bindings, read_decisions)
            }));
        }
        let witness =
            self.prepare_optional_action_witness(action, stack_object, bindings, decisions);
        let first_event = self.state.events.len();
        self.execute_effects(
            &Value::Array(vec![action.clone()]),
            stack_object,
            bindings,
            decisions,
            provider,
        )?;
        Ok(self.optional_action_was_performed(&witness, first_event))
    }

    /// Runs a mandatory instruction; its measured performance selects the
    /// outcome program (`onPerformed`, else `onDeclined`, CR 603.12, 609.3).
    pub(super) fn execute_performed_action<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let performed =
            self.run_measured_instruction(effect, stack_object, bindings, decisions, provider)?;
        let outcome = if performed {
            effect.get("onPerformed")
        } else {
            effect.get("onDeclined")
        };
        if let Some(outcome) = outcome {
            self.execute_effects(outcome, stack_object, bindings, decisions, provider)?;
        }
        Ok(())
    }

    fn prepare_group_measure(
        &self,
        actions: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> GroupMeasure {
        let mut produced = BTreeSet::new();
        let mut members = Vec::new();
        for member in actions.as_array().into_iter().flatten() {
            let measure = match group_role(member) {
                Some(GroupRole::Selection) => MemberMeasure::Selection(member.clone()),
                Some(GroupRole::Neutral) | None => MemberMeasure::Neutral,
                // A member reading an earlier member's result, or one whose
                // performance only the group measures (a reveal, a chosen
                // discard or sacrifice), is measured after the group.
                Some(GroupRole::Measured)
                    if reads_any(member, &produced) || group_measured_member(member) =>
                {
                    MemberMeasure::Deferred(member.clone())
                }
                Some(GroupRole::Measured) => MemberMeasure::Prepared(
                    self.prepare_optional_action_witness(member, stack_object, bindings, decisions),
                ),
            };
            produced_names(member, &mut produced);
            members.push(measure);
        }
        GroupMeasure {
            first_event: self.state.events.len(),
            zones: self.object_zone_snapshot(),
            members,
        }
    }

    /// Every object's current zone.
    fn object_zone_snapshot(&self) -> BTreeMap<String, &'static str> {
        let mut zones = BTreeMap::new();
        for player in &self.state.players {
            let lists: [(&'static str, &Vec<CardInstance>); 7] = [
                ("battlefield", &player.battlefield),
                ("library", &player.library),
                ("hand", &player.hand),
                ("graveyard", &player.graveyard),
                ("exile", &player.exile),
                ("sideboard", &player.sideboard),
                ("commandZone", &player.command_zone),
            ];
            for (zone, cards) in lists {
                for card in cards {
                    zones.insert(card.instance_id.clone(), zone);
                }
            }
        }
        zones
    }

    fn group_was_performed(
        &self,
        measure: &GroupMeasure,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        let mut measured = false;
        for member in &measure.members {
            let performed = match member {
                MemberMeasure::Neutral => true,
                MemberMeasure::Selection(choice) => {
                    let chosen = choice["id"]
                        .as_str()
                        .and_then(|id| decisions.get(id))
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len) as i64;
                    chosen >= self.requested_choice_count(choice, stack_object, bindings, decisions)
                }
                MemberMeasure::Prepared(witness) => {
                    measured = true;
                    self.optional_action_was_performed(witness, measure.first_event)
                }
                MemberMeasure::Deferred(action) => {
                    measured = true;
                    self.deferred_member_performed(
                        action,
                        measure,
                        stack_object,
                        bindings,
                        decisions,
                    )
                }
            };
            if !performed {
                return false;
            }
        }
        measured
    }

    /// The number of objects a choice asks for. A count clamped to its own
    /// population (`minimum(N, countObjects(...))`) asks for N; a bounded
    /// choice asks for its minimum, and at least one object.
    fn requested_choice_count(
        &self,
        choice: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> i64 {
        let evaluate = |expression: &Value| {
            i64::from(
                self.runtime_numeric_expression(expression, stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0),
            )
        };
        if let Some(count) = choice.get("count") {
            if value_kind(count) == Some("minimum")
                && let Some([first, second]) = count["operands"].as_array().map(Vec::as_slice)
            {
                let requested = match (value_kind(first), value_kind(second)) {
                    (_, Some("countObjects")) => Some(first),
                    (Some("countObjects"), _) => Some(second),
                    _ => None,
                };
                if let Some(requested) = requested {
                    return evaluate(requested);
                }
            }
            return evaluate(count);
        }
        evaluate(&choice["minimum"]).max(1)
    }

    /// A member that read an earlier member's result, measured after the
    /// group: the objects it names now, against the zones they occupied
    /// before the group ran.
    fn deferred_member_performed(
        &self,
        action: &Value,
        measure: &GroupMeasure,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        let objects = |field: &str| {
            self.resolve_runtime_object_ids(&action[field], stack_object, bindings, decisions)
                .unwrap_or_default()
        };
        let moved = |ids: Vec<String>| {
            !ids.is_empty()
                && ids.iter().all(|id| {
                    let before = measure.zones.get(id).copied();
                    before.is_some() && self.object_zone_name(id) != before
                })
        };
        match value_kind(action) {
            Some("revealCards") => {
                let ids = objects("cards");
                let revealed = self.state.events
                    [measure.first_event.min(self.state.events.len())..]
                    .iter()
                    .filter(|event| event.kind == "cardsRevealed")
                    .flat_map(|event| event.detail["cardIds"].as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>();
                !ids.is_empty() && ids.iter().all(|id| revealed.contains(id.as_str()))
            }
            Some("discardCards" | "moveCards" | "exileTargetCards") => moved(objects("cards")),
            Some("sacrificePermanents") => moved(objects("objects")),
            Some("destroyPermanent" | "exilePermanent" | "sacrificePermanent") => {
                moved(objects("permanent"))
            }
            Some("returnToOwnersHand") => moved(objects("object")),
            // An event-measured member whose amount an earlier member bound
            // (`draw X cards, where X is ...`): its witness prepared now,
            // over the events the whole group recorded.
            _ => {
                let witness =
                    self.prepare_optional_action_witness(action, stack_object, bindings, decisions);
                match witness {
                    PerformanceWitness::Events { .. } => {
                        self.optional_action_was_performed(&witness, measure.first_event)
                    }
                    // Any other witness reads the state before its action.
                    _ => false,
                }
            }
        }
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
    /// Every permanent is controlled by `controller` afterwards.
    Control {
        objects: Vec<String>,
        controller: Option<String>,
    },
    /// Every attachment, not already attached to `to`, is attached to it
    /// afterwards.
    Attached {
        attachments: Vec<(String, Option<String>)>,
        to: Option<String>,
    },
    /// Every object is still the permanent the effect applied to.
    Present { objects: Vec<String> },
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
            // CR 122.8: the stated number of counters left the one object.
            Some("removeCounters") => {
                let objects = self
                    .counter_removal_instance(action, stack_object, bindings)
                    .into_iter()
                    .collect::<Vec<_>>();
                let required = i64::from(
                    self.counter_removal_count(action, stack_object, bindings, decisions)
                        .max(1),
                );
                PerformanceWitness::Events {
                    kinds: &["counterRemoved"],
                    player: None,
                    objects: Some(objects),
                    measure: EventMeasure::Sum("count"),
                    required,
                }
            }
            // CR 608.2c: the payment instruction's cost was paid.
            Some("payCost") => events(
                &["resolutionCostPaid"],
                player(&action["player"]),
                EventMeasure::Count,
                1,
            ),
            // CR 120.1, 615.1: damage was dealt; prevented damage is not.
            Some("dealDamage") => PerformanceWitness::Events {
                kinds: &["damageDealt"],
                player: None,
                objects: None,
                measure: EventMeasure::Sum("amount"),
                required: 1,
            },
            Some("loseLife") => events(
                &["lifeLost"],
                player(&action["player"]),
                EventMeasure::Sum("amount"),
                amount("amount").max(1),
            ),
            Some("gainControlPermanent") => PerformanceWitness::Control {
                objects: self.optional_action_objects(
                    &action["permanent"],
                    stack_object,
                    bindings,
                    decisions,
                ),
                controller: player(&action["controller"]),
            },
            Some("attachPermanent") => {
                let attachments = match value_kind(&action["attachment"]) {
                    Some("self" | "abilitySource") => vec![stack_object.card.instance_id.clone()],
                    _ => self.optional_action_objects(
                        &action["attachment"],
                        stack_object,
                        bindings,
                        decisions,
                    ),
                };
                PerformanceWitness::Attached {
                    attachments: attachments
                        .into_iter()
                        .map(|id| {
                            let before = self.attached_to(&id);
                            (id, before)
                        })
                        .collect(),
                    to: self
                        .optional_action_objects(&action["to"], stack_object, bindings, decisions)
                        .into_iter()
                        .next(),
                }
            }
            Some("modifyPowerToughness" | "grantKeyword") => {
                let objects = match value_kind(&action["object"]) {
                    Some("self" | "abilitySource") => vec![stack_object.card.instance_id.clone()],
                    _ => self.optional_action_objects(
                        &action["object"],
                        stack_object,
                        bindings,
                        decisions,
                    ),
                };
                PerformanceWitness::Present { objects }
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
            PerformanceWitness::Control {
                objects,
                controller,
            } => {
                let Some(controller) = controller else {
                    return false;
                };
                !objects.is_empty()
                    && objects.iter().all(|id| {
                        self.permanent_position(id)
                            .is_some_and(|(player_index, card_index)| {
                                self.state.players[player_index].battlefield[card_index].controller
                                    == *controller
                            })
                    })
            }
            PerformanceWitness::Attached { attachments, to } => {
                let Some(to) = to else {
                    return false;
                };
                !attachments.is_empty()
                    && attachments.iter().all(|(id, before)| {
                        before.as_deref() != Some(to.as_str())
                            && self.attached_to(id).as_deref() == Some(to.as_str())
                    })
            }
            PerformanceWitness::Present { objects } => {
                !objects.is_empty()
                    && objects
                        .iter()
                        .all(|id| self.permanent_position(id).is_some())
            }
        }
    }

    /// The permanent an attachment is attached to, if it is on the battlefield.
    fn attached_to(&self, instance_id: &str) -> Option<String> {
        self.permanent_position(instance_id)
            .and_then(|(player_index, card_index)| {
                self.state.players[player_index].battlefield[card_index]
                    .attached_to
                    .clone()
            })
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
    pub(super) fn object_zone_name(&self, instance_id: &str) -> Option<&'static str> {
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
