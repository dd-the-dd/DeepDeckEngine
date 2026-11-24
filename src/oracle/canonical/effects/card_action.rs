//! Lexical card-collection bindings shared by ordinary action sequences.
use super::super::*;

#[derive(Clone, Default)]
pub(in crate::oracle::canonical) struct CardCollectionMemory {
    view: Option<Value>,
    view_maximum: Option<i64>,
    /// The verb that produced `view` when a later `VERBed this way` restates it.
    view_verb: Option<CardCollectionVerb>,
    /// The zone holding the observed cards when the producing action proves
    /// it (drawn cards are in hand). Readers needing a zone require it.
    view_zone: Option<CardZoneName>,
    /// The view is a whole zone named by a singular noun (`their hand`), so
    /// `from it` may name the collection, not one card.
    view_zone_noun: bool,
    selected: Vec<Value>,
    complement_basis: Option<(Value, Option<i64>)>,
    complement_exclusions: Vec<Value>,
    singular_focus: Option<(Value, Option<i64>)>,
    plural_focus: Option<(Value, Option<i64>)>,
    allocated_ids: std::collections::BTreeSet<String>,
    // A lower bound on how many members were removed, bounded by view size.
    removed_up_to_size: i64,
    next_view: usize,
    next_selection: usize,
    next_order: usize,
    next_export: usize,
    next_zone_target: usize,
    next_card_target: usize,
}

/// The card collection this program put onto the battlefield; `it` after
/// that move reads this binding. One battlefield move per program.
pub(in crate::oracle::canonical) const MOVED_ONTO_BATTLEFIELD_BINDING: &str =
    "movedOntoBattlefield";

fn fresh(base: &str, next: &mut usize) -> String {
    *next += 1;
    if *next == 1 {
        base.to_owned()
    } else {
        format!("{base}{next}")
    }
}

fn value_kind_name(value: &Value) -> Option<&str> {
    value.get("kind").and_then(Value::as_str)
}
fn bound(name: &str) -> Value {
    json!({"kind":"boundObjects", "binding":name})
}
fn count(objects: Value) -> Value {
    json!({"kind":"countObjects", "objects":objects})
}
fn difference(left: Value, right: Value) -> Value {
    json!({"kind":"setDifference", "left":left, "right":right})
}

impl CardCollectionMemory {
    /// Claims the one moved-onto-battlefield binding of this program (the
    /// same single allocation a battlefield-entry card move takes). `false`
    /// when another producer already holds it.
    pub(in crate::oracle::canonical) fn allocate_moved_onto_battlefield(&mut self) -> bool {
        self.allocated_ids
            .insert(MOVED_ONTO_BATTLEFIELD_BINDING.to_string())
    }

    /// Selection bindings share one allocator across nested program scopes.
    pub(in crate::oracle::canonical) fn allocate_selection_id(&mut self, prefix: &str) -> String {
        let id = fresh(prefix, &mut self.next_selection);
        self.allocated_ids.insert(id.clone());
        id
    }
    /// The singular card result of the current scope, if one is proven: the
    /// single looked-at card or the result of a one-card choice.
    pub(in crate::oracle::canonical) fn singular_focus_object(&self) -> Option<Value> {
        self.view.as_ref()?;
        self.singular_focus
            .as_ref()
            .filter(|(_, maximum)| maximum.is_some_and(|maximum| maximum <= 1))
            .map(|(value, _)| value.clone())
    }

    /// Restores the antecedent scope of `snapshot` (a conditional's other
    /// branch reads the antecedents before the first branch) while keeping
    /// this allocator's identifiers, so the branches never share an ID.
    pub(in crate::oracle::canonical) fn restore_scope(&mut self, snapshot: &Self) {
        self.view = snapshot.view.clone();
        self.view_maximum = snapshot.view_maximum;
        self.view_verb = snapshot.view_verb;
        self.view_zone = snapshot.view_zone;
        self.view_zone_noun = snapshot.view_zone_noun;
        self.selected = snapshot.selected.clone();
        self.complement_basis = snapshot.complement_basis.clone();
        self.complement_exclusions = snapshot.complement_exclusions.clone();
        self.singular_focus = snapshot.singular_focus.clone();
        self.plural_focus = snapshot.plural_focus.clone();
        self.removed_up_to_size = snapshot.removed_up_to_size;
    }

    /// Whether two memories expose the same antecedent scope.
    pub(in crate::oracle::canonical) fn same_scope(&self, other: &Self) -> bool {
        self.view == other.view
            && self.view_maximum == other.view_maximum
            && self.view_verb == other.view_verb
            && self.view_zone == other.view_zone
            && self.view_zone_noun == other.view_zone_noun
            && self.selected == other.selected
            && self.complement_basis == other.complement_basis
            && self.complement_exclusions == other.complement_exclusions
            && self.singular_focus == other.singular_focus
            && self.plural_focus == other.plural_focus
            && self.removed_up_to_size == other.removed_up_to_size
    }

    pub(in crate::oracle::canonical) fn clear_antecedent(&mut self) {
        self.view = None;
        self.selected.clear();
        self.complement_basis = None;
        self.complement_exclusions.clear();
        self.view_maximum = None;
        self.view_verb = None;
        self.view_zone = None;
        self.view_zone_noun = false;
        self.singular_focus = None;
        self.plural_focus = None;
        self.removed_up_to_size = 0;
    }

    /// A player's revealed hand (CR 701.20a) is the observed collection: its
    /// identities are fixed when revealed (CR 608.2c), and `from it` names
    /// the whole hand. Returns the binding that records those cards.
    pub(in crate::oracle::canonical) fn observe_revealed_hand(&mut self) -> String {
        let id = fresh("revealedHand", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, None);
        self.view_verb = Some(CardCollectionVerb::Revealed);
        self.view_zone = Some(CardZoneName::Hand);
        self.view_zone_noun = true;
        id
    }

    /// The cards a lexical reference (`that card`, `them`) names for an action
    /// performed by another lowering family, such as a player's discard.
    pub(in crate::oracle::canonical) fn referenced_cards(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<Value> {
        let value = self.reference(cursor, reference)?;
        self.mention(value.clone());
        Ok(value.0)
    }

    /// The cards a mill just put into a graveyard become the observed
    /// collection (`mill four cards, then put a creature card from among them
    /// into your hand`). Returns the binding the mill primitive records.
    pub(in crate::oracle::canonical) fn observe_milled_cards(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("milledCards", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_zone = Some(CardZoneName::Graveyard);
        id
    }

    /// The cards a controller draw just put into hand become the observed
    /// collection (`draw three cards and reveal them`). Returns the binding
    /// the draw primitive must record.
    pub(in crate::oracle::canonical) fn observe_drawn_cards(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("drawnCards", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_zone = Some(CardZoneName::Hand);
        id
    }

    /// The permanent a scalar exile instruction just moved is the observed
    /// collection of its program: a later `that card` or `it` reads the
    /// object it became in exile (CR 400.7, 610.3). Returns the binding the
    /// exile primitive must record.
    pub(in crate::oracle::canonical) fn observe_exiled_object(&mut self) -> String {
        self.observe_exiled_objects(Some(1))
    }

    /// The cards an exile of declared targets produced (`exile any number
    /// of target ..., then return those cards`): the same observed exile
    /// view as one object, with the declaration's maximum when it is a
    /// literal and no fixed size otherwise.
    pub(in crate::oracle::canonical) fn observe_exiled_objects(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("exiledObject", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_verb = Some(CardCollectionVerb::Exiled);
        self.view_zone = Some(CardZoneName::Exile);
        id
    }

    fn mention(&mut self, value: (Value, Option<i64>)) {
        if value.1.is_some_and(|maximum| maximum <= 1) {
            self.singular_focus = Some(value);
        } else {
            self.plural_focus = Some(value);
            self.singular_focus = None;
        }
    }

    fn reference(
        &self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<(Value, Option<i64>)> {
        // `the exiled card(s)` restates this program's exile; without one it
        // names the cards a linked ability exiled (CR 607.2a), proved by the
        // card-composition pass. `cards exiled with this` is always linked.
        // `exiled with this` reads every exile of the object (`face`); `the
        // exiled card(s)` names the one exile it is paired with (`pair`).
        match reference {
            CardCollectionReferenceNode::Linked => {
                return Ok((
                    json!({"kind":"cardsExiledWithSource", "link": "face"}),
                    None,
                ));
            }
            CardCollectionReferenceNode::Exiled
                if self.view.is_none() || self.view_verb != Some(CardCollectionVerb::Exiled) =>
            {
                return Ok((
                    json!({"kind":"cardsExiledWithSource", "link": "pair"}),
                    None,
                ));
            }
            CardCollectionReferenceNode::Exiled => {
                return self
                    .singular_focus
                    .clone()
                    .or_else(|| self.plural_focus.clone())
                    .ok_or_else(|| cursor.fail("the cards this program exiled"));
            }
            _ => {}
        }
        // Without an observed collection, `it` / `that card` is the card the
        // triggering zone change put into a public zone (CR 400.7).
        if self.view.is_none()
            && matches!(
                reference,
                CardCollectionReferenceNode::Selected | CardCollectionReferenceNode::Antecedent
            )
            && cursor
                .binding_environment()
                .is_some_and(|environment| environment.frame.event_card.is_some())
        {
            return Ok((json!({"kind":"triggeringCard"}), Some(1)));
        }
        self.view.as_ref().ok_or_else(|| {
            cursor.fail("an observed card collection in the current lexical scope")
        })?;
        match reference {
            CardCollectionReferenceNode::Antecedent => self.plural_focus.clone().ok_or_else(|| {
                cursor.fail("a plural card-collection antecedent in the current scope")
            }),
            CardCollectionReferenceNode::Selected => self
                .singular_focus
                .clone()
                .ok_or_else(|| cursor.fail("a singular card antecedent in the current scope")),
            CardCollectionReferenceNode::Exiled | CardCollectionReferenceNode::Linked => {
                unreachable!("linked and exiled references are resolved above")
            }
            CardCollectionReferenceNode::Zone => {
                cursor.require(
                    self.view_zone_noun,
                    "an observed zone named by a singular noun (a revealed hand) for `from it`",
                )?;
                Ok((
                    self.view.clone().expect("checked observed collection"),
                    self.view_maximum,
                ))
            }
            CardCollectionReferenceNode::Remainder | CardCollectionReferenceNode::Other => {
                cursor.require(
                    !self.complement_exclusions.is_empty(),
                    "a preceding selection for the remainder reference",
                )?;
                let (basis, basis_maximum) = self
                    .complement_basis
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the source scope of the preceding selection"))?;
                let maximum = basis_maximum
                    .map(|maximum| maximum.saturating_sub(self.removed_up_to_size).max(0));
                if reference == CardCollectionReferenceNode::Other {
                    cursor.require(
                        maximum.is_some_and(|maximum| maximum <= 1),
                        "a statically bounded singular complement for the other card",
                    )?;
                }
                Ok((
                    self.complement_exclusions
                        .iter()
                        .cloned()
                        .fold(basis.clone(), difference),
                    maximum,
                ))
            }
        }
    }

    /// Optional outputs are real typed empty collections on the declined path,
    /// never an absent decision interpreted as empty. The producing body stays
    /// inside permission. An optional new observation cannot escape this scope.
    pub(in crate::oracle::canonical) fn export_optional_outputs(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        before: &Self,
        body: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.require(
            self.view == before.view,
            "an optional block whose observation scope does not escape conditionally",
        )?;
        // Local IDs from old children are not globally allocated yet. Refuse
        // such nested output rather than aliasing an outer choice accidentally.
        fn ids_are_allocated(
            value: &Value,
            allocated: &std::collections::BTreeSet<String>,
        ) -> bool {
            match value {
                Value::Array(items) => items.iter().all(|item| ids_are_allocated(item, allocated)),
                Value::Object(object) => {
                    object
                        .get("id")
                        .and_then(Value::as_str)
                        .is_none_or(|id| allocated.contains(id))
                        && object
                            .values()
                            .all(|item| ids_are_allocated(item, allocated))
                }
                _ => true,
            }
        }
        cursor.require(
            body.iter()
                .all(|effect| ids_are_allocated(effect, &self.allocated_ids)),
            "shared lexical ID allocation for every nested optional decision",
        )?;
        let Some(view) = self.view.clone() else {
            return Ok(Vec::new());
        };
        let mut initialize = Vec::new();
        let mut exports = Vec::<(Value, Value)>::new();
        for index in before.selected.len()..self.selected.len() {
            let original = self.selected[index].clone();
            let id = fresh("optionalCards", &mut self.next_export);
            self.allocated_ids.insert(id.clone());
            initialize.push(
                json!({"kind":"bind", "id":id, "value":difference(view.clone(),view.clone())}),
            );
            body.push(json!({"kind":"bind", "id":id, "value":original}));
            self.selected[index] = bound(&id);
            exports.push((original, self.selected[index].clone()));
        }
        for (current, prior) in [
            (&mut self.singular_focus, &before.singular_focus),
            (&mut self.plural_focus, &before.plural_focus),
        ] {
            if current == prior {
                continue;
            }
            let Some((value, maximum)) = current.as_mut() else {
                continue;
            };
            if let Some((_, replacement)) = exports.iter().find(|(original, _)| original == value) {
                *value = replacement.clone();
            } else {
                let id = fresh("optionalCards", &mut self.next_export);
                self.allocated_ids.insert(id.clone());
                initialize.push(
                    json!({"kind":"bind", "id":id, "value":difference(view.clone(),view.clone())}),
                );
                body.push(json!({"kind":"bind", "id":id, "value":value}));
                *value = bound(&id);
            }
            let _ = maximum;
        }
        fn replace_export_references(value: &mut Value, exports: &[(Value, Value)]) {
            if let Some((_, replacement)) = exports.iter().find(|(original, _)| original == value) {
                *value = replacement.clone();
                return;
            }
            match value {
                Value::Array(items) => {
                    for item in items {
                        replace_export_references(item, exports);
                    }
                }
                Value::Object(object) => {
                    for item in object.values_mut() {
                        replace_export_references(item, exports);
                    }
                }
                _ => {}
            }
        }
        let same_basis = self.complement_basis == before.complement_basis;
        if let Some((basis, _)) = &mut self.complement_basis {
            replace_export_references(basis, &exports);
        }
        for exclusion in &mut self.complement_exclusions {
            replace_export_references(exclusion, &exports);
        }
        self.removed_up_to_size = if same_basis {
            before.removed_up_to_size
        } else {
            0
        };
        Ok(initialize)
    }

    /// A freshly produced collection becomes the sole antecedent scope: its
    /// remainder, singular and plural references all read this one binding.
    fn observe(&mut self, id: &str, maximum: Option<i64>) {
        self.clear_antecedent();
        self.view = Some(bound(id));
        self.view_maximum = maximum;
        self.complement_basis = Some((bound(id), maximum));
        self.mention((bound(id), maximum));
        // A one-card observation can still be referenced as a collection.
        self.plural_focus = Some((bound(id), maximum));
    }

    fn operand(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        operand: &CardOperandNode,
        declarations: TargetlessDeclarationSupport,
        effects: &mut Vec<Value>,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<(Value, Option<i64>)> {
        if let CardOperandNode::Targets(target) = operand {
            validate_object_predicate_numeric_expressions(&target.predicate, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            cursor.require(
                target.zone == CardZoneName::Graveyard,
                "an admitted target-card source zone",
            )?;
            let zone = match target.owner {
                CardTargetZoneOwner::Player(LocalPlayerReference::Controller) => {
                    graveyard(controller())
                }
                CardTargetZoneOwner::AnyPlayer | CardTargetZoneOwner::OnePlayer => {
                    json!({"kind":"anyGraveyard"})
                }
                _ => return Err(cursor.fail("a bound target-card zone owner")),
            };
            let id = fresh("targetCardOperand", &mut self.next_card_target);
            self.allocated_ids.insert(id.clone());
            let mut decision = json!({"id":id,"kind":"chooseTargets",
                "minimum":lower_target_cardinality_bound(target.cardinality.minimum),
                "maximum":lower_target_cardinality_bound(target.cardinality.maximum),
                "candidates":{"kind":"cards","zone":zone,"where":lower_card_object_predicate(&target.predicate)}});
            // `another target ... card`: the source card is not a candidate.
            if target.exclude_source {
                decision["candidates"]["excludeSource"] = json!(true);
            }
            // `any number of target ... cards` (CR 601.2c): the shared count.
            if target.unbounded {
                let applied = apply_target_count_to_decision(&mut decision, &target.count());
                cursor
                    .require_value(applied, "an unbounded card-target count the IR represents")?;
            }
            if target.owner == CardTargetZoneOwner::OnePlayer
                && (target.unbounded
                    || target.cardinality != ExpressedTargetCardinality::EXACTLY_ONE)
            {
                decision["selectionConstraint"] =
                    json!({"kind":"sameZoneOwner","zone":target.zone.ir_name()});
            }
            target_decisions.push(decision);
            self.clear_antecedent();
            let maximum = match target.cardinality.maximum {
                NumericAtom::Integer(value) if !target.unbounded => Some(value),
                _ => None,
            };
            return Ok((json!({"kind":"chosenTargets","id":id}), maximum));
        }
        if let CardOperandNode::LibraryTop(top) = operand {
            let player = match &top.owner {
                CardLibraryOwnerNode::Player(LocalPlayerReference::Controller) => controller(),
                CardLibraryOwnerNode::Player(_) => {
                    return Err(cursor.fail("a supported explicit library owner"));
                }
                // `the top N cards of target opponent's library`: the
                // declared player owns the library the cards are read from.
                CardLibraryOwnerNode::TargetPlayer(targets) => {
                    let [slot] = targets.slots.as_slice() else {
                        return Err(cursor.fail("one library-owner target slot"));
                    };
                    let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                        .ok_or_else(|| {
                            cursor.fail("a player target selector for the library owner")
                        })?;
                    cursor.require(
                        matches!(slot.selector, TargetSelector::Player(_))
                            && slot.cardinality == TargetCardinality::EXACTLY_ONE,
                        "a singular targeted player as library owner",
                    )?;
                    let id = fresh("targetZonePlayer", &mut self.next_zone_target);
                    self.allocated_ids.insert(id.clone());
                    target_decisions.push(target_decision(&id, candidates, 1, 1));
                    chosen_target(&id)
                }
            };
            let count_ir = lower_targetless_numeric_expression(&top.count, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            let maximum = match &top.count {
                NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                _ => None,
            };
            let id = fresh("topCards", &mut self.next_view);
            self.allocated_ids.insert(id.clone());
            self.observe(&id, maximum);
            // The set is fixed when the action starts (CR 608.2c): later verbs
            // read these identities, not whatever is on top afterwards.
            effects.push(json!({"kind":"bind", "id":id,
                "value":{"kind":"libraryTopCards",
                    "zone":{"kind":"library", "player":player}, "count":count_ir}}));
            return Ok((bound(&id), maximum));
        }
        if let CardOperandNode::ZoneCollection(query) = operand {
            cursor.require(
                matches!(
                    query.zone,
                    CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
                ),
                "an executable hand, graveyard or exile collection",
            )?;
            let player = match &query.owner {
                CardZoneOwnerNode::Player(LocalPlayerReference::Controller) => controller(),
                CardZoneOwnerNode::Player(_) => {
                    return Err(cursor.fail("a resolved collection owner reference"));
                }
                CardZoneOwnerNode::TargetPlayer(targets) => {
                    let [slot] = targets.slots.as_slice() else {
                        return Err(cursor.fail("one zone-owner target slot"));
                    };
                    let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                        .ok_or_else(|| {
                            cursor.fail("a player target selector for the zone owner")
                        })?;
                    cursor.require(
                        matches!(slot.selector, TargetSelector::Player(_))
                            && slot.cardinality == TargetCardinality::EXACTLY_ONE,
                        "a singular targeted player as zone owner",
                    )?;
                    let id = fresh("targetZonePlayer", &mut self.next_zone_target);
                    self.allocated_ids.insert(id.clone());
                    target_decisions.push(target_decision(&id, candidates, 1, 1));
                    chosen_target(&id)
                }
            };
            let mut objects =
                json!({"kind":"cardsInZone","zone":{"kind":query.zone.ir_name(),"player":player}});
            if let Some(predicate) = &query.predicate {
                objects["where"] = lower_card_object_predicate(predicate)
            }
            if let Some(quantity) = &query.quantity {
                // The acting player chooses on resolution (no target): as many
                // matching cards as stated, or all of them if fewer exist.
                let NumericExpression::Atom(NumericAtom::Integer(fixed)) = quantity else {
                    return Err(cursor.fail("a literal number of cards chosen from the zone"));
                };
                cursor.require(*fixed > 0, "a positive number of chosen cards")?;
                let id = fresh("chosenZoneCards", &mut self.next_selection);
                self.allocated_ids.insert(id.clone());
                effects.push(json!({"kind":"chooseCards", "id":id, "player":controller(),
                    "from":objects.clone(),
                    "count":{"kind":"minimum", "operands":[integer(*fixed), count(objects)]}}));
                self.clear_antecedent();
                let result = decision_result(&id);
                self.mention((result.clone(), Some(*fixed)));
                return Ok((result, Some(*fixed)));
            }
            // A named zone is not an observation or selection result. Never
            // inherit an unrelated earlier look as its later complement.
            self.clear_antecedent();
            return Ok((objects, None));
        }
        let CardOperandNode::Selection(selection) = operand else {
            let CardOperandNode::Reference(reference) = operand else {
                unreachable!()
            };
            let value = self.reference(cursor, *reference)?;
            self.mention(value.clone());
            return Ok(value);
        };
        let (from, source_maximum) = self.reference(cursor, selection.from)?;
        if self.complement_basis.as_ref().map(|(basis, _)| basis) != Some(&from) {
            self.complement_basis = Some((from.clone(), source_maximum));
            self.complement_exclusions.clear();
            self.removed_up_to_size = 0;
        }
        // A bare `card` noun is the domain itself and filters nothing; any
        // other predicate keeps its lowering, never silently dropped.
        let predicate = selection
            .predicate
            .as_ref()
            .filter(|predicate| **predicate != ObjectPredicate::ObjectKind(ObjectKind::Card))
            .map(lower_card_object_predicate);
        if let CardSelectionQuantityNode::All { via } = &selection.quantity {
            // No player choice: the selected set is the filtered view itself,
            // and `the rest` is whatever this set leaves in the view.
            if let Some(via) = via {
                cursor.require(
                    self.view_verb == Some(*via),
                    "a collection produced by the verb restated with `this way`",
                )?;
            }
            let result = predicate.as_ref().map_or_else(
                || from.clone(),
                |predicate| json!({"kind":"filterObjects", "objects":from, "where":predicate}),
            );
            self.selected.push(result.clone());
            self.complement_exclusions.push(result.clone());
            let maximum = if predicate.is_none() {
                source_maximum
            } else {
                None
            };
            self.mention((result.clone(), maximum));
            return Ok((result, maximum));
        }
        let filtered = predicate.as_ref().map_or_else(
            || from.clone(),
            |predicate| json!({"kind":"filterObjects", "objects":from, "where":predicate}),
        );
        let available = count(filtered);
        let (minimum, maximum, exact, fixed) = match &selection.quantity {
            CardSelectionQuantityNode::Exactly(amount)
            | CardSelectionQuantityNode::UpTo(amount) => {
                let value = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let fixed = match amount {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => {
                        cursor.require(*value >= 0, "a nonnegative selection quantity")?;
                        Some(*value)
                    }
                    _ => None,
                };
                let bounded = json!({"kind":"minimum", "operands":[value,available]});
                let exact = matches!(selection.quantity, CardSelectionQuantityNode::Exactly(_));
                (
                    if exact { bounded.clone() } else { integer(0) },
                    bounded,
                    exact,
                    fixed,
                )
            }
            CardSelectionQuantityNode::AnyNumber => (integer(0), available, false, None),
            CardSelectionQuantityNode::All { .. } => {
                unreachable!("an unconditional selection returns before any player choice")
            }
        };
        let id = fresh("cardsForHand", &mut self.next_selection);
        self.allocated_ids.insert(id.clone());
        let mut choose = json!({"kind":"chooseCards", "id":id, "player":controller(), "from":from});
        if exact {
            choose["count"] = maximum;
        } else {
            choose["minimum"] = minimum;
            choose["maximum"] = maximum;
        }
        if let Some(predicate) = predicate {
            choose["where"] = predicate;
        }
        effects.push(choose);
        let result = decision_result(&id);
        self.selected.push(result.clone());
        self.complement_exclusions.push(result.clone());
        self.mention((result.clone(), fixed));
        if exact
            && selection.predicate.is_none()
            && self.complement_basis.as_ref().map(|(basis, _)| basis) == Some(&from)
        {
            // Only the first unfiltered choice proves a lower bound. Later
            // independent choices may overlap; do not sum them as disjoint.
            self.removed_up_to_size = self.removed_up_to_size.max(fixed.unwrap_or(0));
        }
        Ok((result, fixed))
    }

    /// CR 608.2g: `you may cast N of them without paying` casts during this
    /// resolution, from the zone the observed cards are proved to be in.
    fn lower_resolution_cast(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        permit: &CardPermissionNode,
        effects: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.enter("lower.card-action.resolution-cast");
        cursor.require(
            permit.verb == CardPermissionVerb::Cast,
            "`cast`: playing a land is not a resolution-time free cast",
        )?;
        let (cards, maximum) = match &permit.cards {
            CardOperandNode::Reference(reference) => self.reference(cursor, *reference)?,
            CardOperandNode::Selection(selection) => {
                let (from, _) = self.reference(cursor, selection.from)?;
                let maximum = match &selection.quantity {
                    CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                        NumericAtom::Integer(value),
                    ))
                    | CardSelectionQuantityNode::UpTo(NumericExpression::Atom(
                        NumericAtom::Integer(value),
                    )) if *value > 0 => Some(*value),
                    CardSelectionQuantityNode::AnyNumber => None,
                    _ => {
                        return Err(cursor
                            .fail("a positive literal or `any number` of observed cards to cast"));
                    }
                };
                let cards = match &selection.predicate {
                    Some(predicate) => json!({"kind":"filterObjects", "objects":from,
                        "where":lower_card_object_predicate(predicate)}),
                    None => from,
                };
                (cards, maximum)
            }
            _ => return Err(cursor.fail("an observed card collection to cast from")),
        };
        let zone = self.view_zone.ok_or_else(|| {
            cursor.fail("observed cards whose zone (hand, graveyard or exile) is proved")
        })?;
        cursor.require(
            matches!(
                zone,
                CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
            ),
            "a hand, graveyard or exile source zone for the cast",
        )?;
        let mut cast = json!({
            "kind": "castAnyNumber",
            "player": controller(),
            "cards": cards,
            "where": {"kind": "canBeCastAsSpell"},
            "timing": {"kind": "duringResolution"},
            "withoutPayingManaCost": true,
            "alternativeCostsAllowed": false,
            "additionalCostsApply": true,
            "variableManaValue": integer(0),
            "sourceZone": zone.ir_name(),
        });
        if let Some(maximum) = maximum {
            cast["maximum"] = integer(maximum);
        }
        effects.push(cast);
        Ok(std::mem::take(effects))
    }

    pub(in crate::oracle::canonical) fn lower_action(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        action: &CardActionNode,
        declarations: TargetlessDeclarationSupport,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.enter("lower.card-action.bindings");
        let mut effects = Vec::new();
        match action {
            CardActionNode::Look {
                count: amount,
                owner,
            } => {
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "a supported explicit library owner",
                )?;
                let amount_ir = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let id = fresh("lookedCards", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                let maximum = match amount {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                    _ => None,
                };
                self.observe(&id, maximum);
                effects.push(json!({"kind":"lookAtTopCards", "zone":{"kind":"library", "player":controller()}, "count":amount_ir, "bind":id}));
            }
            CardActionNode::Reveal(operand) => {
                let (cards, _) = self.operand(
                    cursor,
                    operand,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                effects.push(json!({"kind":"revealCards", "cards":cards}));
                if matches!(operand, CardOperandNode::LibraryTop(_)) {
                    self.view_verb = Some(CardCollectionVerb::Revealed);
                }
            }
            CardActionNode::RevealUntil {
                quantity,
                predicate,
                owner,
            } => {
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "a supported explicit library owner",
                )?;
                let count_ir = lower_targetless_numeric_expression(quantity, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let maximum = match quantity {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                    _ => None,
                };
                let id = fresh("revealedCards", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                let matching = json!({"kind":"filterObjects", "objects":bound(&id),
                    "where":lower_card_object_predicate(predicate)});
                effects.push(json!({"kind":"revealFromTopUntilCount",
                    "zone":{"kind":"library", "player":controller()},
                    "count":count_ir, "where":lower_card_object_predicate(predicate), "bind":id}));
                // Every revealed card is observed; the matching cards are what a
                // later `that card` names and what `the rest` excludes.
                self.observe(&id, None);
                self.view_verb = Some(CardCollectionVerb::Revealed);
                self.selected.push(matching.clone());
                self.complement_exclusions.push(matching.clone());
                self.mention((matching, maximum));
            }
            CardActionNode::Choose(selection) => {
                cursor.enter("lower.card-action.choose");
                // The choice itself is the action: the operand child emits
                // `chooseCards` and records its result as the antecedent.
                self.operand(
                    cursor,
                    &CardOperandNode::Selection(selection.clone()),
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
            }
            CardActionNode::Shuffle { owner } => {
                cursor.enter("lower.card-action.shuffle");
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "the controller's library",
                )?;
                // Shuffled cards are new unordered identities for later text.
                self.clear_antecedent();
                effects.push(json!({"kind":"shuffleZone",
                    "zone":{"kind":"library", "player":controller()}}));
            }
            CardActionNode::Permit(permit)
                if permit.without_paying_mana_cost && permit.duration.is_none() =>
            {
                return self.lower_resolution_cast(cursor, permit, &mut effects);
            }
            CardActionNode::Permit(permit) => {
                let (cards, maximum) = self.operand(
                    cursor,
                    &permit.cards,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                cursor.enter("lower.card-action.permit");
                let permits_exiled_view = self.view_verb == Some(CardCollectionVerb::Exiled)
                    && Some(&cards) == self.view.as_ref();
                let card = match value_kind_name(&cards) {
                    Some("boundObjects") if maximum.is_some_and(|maximum| maximum <= 1) => {
                        json!({"kind":"boundObject", "binding":cards["binding"]})
                    }
                    Some("boundObjects" | "decisionResult") => cards,
                    _ => {
                        return Err(cursor.fail("a bound card collection as the permission object"));
                    }
                };
                if permit.without_paying_mana_cost {
                    return Err(cursor.engine_gap(
                        crate::oracle::canonical::engine_gap::EngineGap::PermissionWindow,
                        "a free casting permission window the runtime represents",
                    ));
                }
                let duration = match &permit.duration {
                    // CR 611.2a + 400.7: an unbounded permission over cards
                    // this program exiled lasts while they remain in exile.
                    None => {
                        cursor.enter("lower.card-action.permit.unbounded");
                        cursor.require(
                            permits_exiled_view,
                            "an unbounded permission over the cards this program exiled",
                        )?;
                        json!({"kind":"whileInExile"})
                    }
                    Some(EffectDurationNode::UntilEndOfCurrentTurn) => {
                        json!({"kind":"untilEndOfCurrentTurn"})
                    }
                    Some(EffectDurationNode::UntilEndOfNextTurn {
                        player: LocalPlayerReference::Controller,
                    }) => json!({"kind":"untilEndOfNextTurn", "player":controller()}),
                    _ => {
                        return Err(cursor.engine_gap(
                            crate::oracle::canonical::engine_gap::EngineGap::PermissionWindow,
                            "a permission window the runtime can end: this turn or the end of your next turn",
                        ));
                    }
                };
                effects.push(json!({"kind":"grantPermission", "player":controller(),
                    "action":{"kind":match permit.verb {
                            CardPermissionVerb::Play => "play",
                            CardPermissionVerb::Cast => "cast",
                        },
                        "card":card, "normalTimingApplies":true, "normalCostsApply":true},
                    "duration":duration}));
            }
            CardActionNode::Move {
                cards: operand,
                destination,
            } => {
                let (mut cards, maximum) = self.operand(
                    cursor,
                    operand,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                let player = match &destination.owner {
                    CardMoveOwnerNode::Player(LocalPlayerReference::Controller) => controller(),
                    // Exile in each owner's exile zone, or the battlefield
                    // under each owner's control (CR 110.2a).
                    // A hand, library or graveyard is always its owner's
                    // (CR 400.3); exile and the battlefield name it.
                    CardMoveOwnerNode::EachCardOwner => {
                        json!({"kind":"ownerOf","object":{"kind":"candidate"}})
                    }
                    _ => return Err(cursor.fail("a supported explicit destination owner")),
                };
                let mut to = json!({"kind":destination.zone.ir_name(), "player":player});
                if destination.zone == CardZoneName::Exile {
                    to["faceDown"] = json!(false)
                }
                if let Some(position) = destination.position {
                    to["position"] = json!(match position {
                        LibraryPosition::Top => "top",
                        LibraryPosition::Bottom => "bottom",
                    });
                }
                if matches!(operand, CardOperandNode::Targets(_))
                    && destination.zone == CardZoneName::Library
                    && destination.position == Some(LibraryPosition::Top)
                    && destination.order == Some(CollectionOrder::Random)
                    && !maximum.is_some_and(|maximum| maximum <= 1)
                {
                    return Err(cursor.fail(
                        "a proved random-order library-top writer for multiple targeted cards",
                    ));
                }
                let mut order = None;
                match destination.order {
                    Some(CollectionOrder::Chosen) => {
                        let id = fresh("remainderOrder", &mut self.next_order);
                        self.allocated_ids.insert(id.clone());
                        effects.push(json!({"kind":"chooseOrder", "id":id, "player":controller(), "objects":cards}));
                        cards = decision_result(&id);
                        order = Some(json!({"kind":"decisionOrder", "decisionId":id}));
                    }
                    Some(CollectionOrder::Random) => order = Some(json!({"kind":"random"})),
                    None => cursor.require(
                        destination.zone != CardZoneName::Library
                            || maximum.is_some_and(|maximum| maximum <= 1),
                        "an explicit ordering policy for a multi-card library placement",
                    )?,
                }
                // Battlefield movement is admitted for cards whose identities
                // are proved: a choice from an observed collection, declared
                // targets, the object an exile just produced, or a zone query
                // evaluated on resolution. Other observed references (cards
                // revealed until a match) still lack a proved runtime writer.
                // The engine applies entry replacements (CR 614.12).
                if destination.zone == CardZoneName::Battlefield {
                    cursor.enter("lower.card-action.battlefield-entry");
                    let proved_operand = match operand {
                        CardOperandNode::Selection(_)
                        | CardOperandNode::Targets(_)
                        | CardOperandNode::ZoneCollection(_) => true,
                        // The card a zone change put into a public zone.
                        CardOperandNode::Reference(_)
                            if matches!(
                                value_kind_name(&cards),
                                Some("triggeringCard" | "cardsExiledWithSource")
                            ) =>
                        {
                            true
                        }
                        CardOperandNode::Reference(_) => {
                            self.view_verb == Some(CardCollectionVerb::Exiled)
                                && self.view_zone == Some(CardZoneName::Exile)
                        }
                        CardOperandNode::LibraryTop(_) => false,
                    };
                    cursor.require(
                        proved_operand
                            && !self.allocated_ids.contains(MOVED_ONTO_BATTLEFIELD_BINDING),
                        "one battlefield entry of chosen, targeted, exiled or zone-queried cards",
                    )?;
                    to["tapped"] = json!(destination.tapped);
                    if let Some(counters) = &destination.entering_counters {
                        let count =
                            lower_targetless_numeric_expression(&counters.count, declarations)
                                .map_err(|error| cursor.fail(error.expectation()))?;
                        to["enterWithCounters"] =
                            json!([{"counter": counters.counter.as_str(), "count": count}]);
                    }
                    self.allocated_ids
                        .insert(MOVED_ONTO_BATTLEFIELD_BINDING.to_string());
                    effects.push(json!({"kind":"moveCards", "cards":cards, "to":to,
                        "bind":MOVED_ONTO_BATTLEFIELD_BINDING}));
                    return Ok(effects);
                }
                cursor.require(
                    !destination.tapped && destination.entering_counters.is_none(),
                    "an untapped, counterless nonbattlefield destination",
                )?;
                if matches!(operand, CardOperandNode::LibraryTop(_))
                    && destination.zone == CardZoneName::Exile
                {
                    self.view_verb = Some(CardCollectionVerb::Exiled);
                }
                let mut movement = json!({"kind":"moveCards", "cards":cards, "to":to});
                if let Some(order) = order {
                    movement["order"] = order;
                }
                effects.push(movement);
            }
        }
        Ok(effects)
    }
}

#[cfg(test)]
#[path = "library_top_operand_tests.rs"]
mod library_top_operand_tests;
