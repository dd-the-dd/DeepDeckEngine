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
    /// The cards the most recent reveal of this scope revealed (`the
    /// revealed cards`).
    last_revealed: Option<(Value, Option<i64>)>,
    /// The cards the most recent exile of this scope moved (`the exiled
    /// card`), when that exile is an action of this program.
    last_exiled: Option<(Value, Option<i64>)>,
    /// The cards a resolution-time cast of this scope cast (`that weren't
    /// cast`), bound by the cast primitive.
    last_cast: Option<Value>,
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
    next_cast: usize,
    next_permission: usize,
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
        self.last_revealed = snapshot.last_revealed.clone();
        self.last_exiled = snapshot.last_exiled.clone();
        self.last_cast = snapshot.last_cast.clone();
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
            && self.last_revealed == other.last_revealed
            && self.last_exiled == other.last_exiled
            && self.last_cast == other.last_cast
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
        self.last_revealed = None;
        self.last_exiled = None;
        self.last_cast = None;
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
            // `the exiled card(s)` after an exile action of this program
            // names exactly the cards that action moved.
            CardCollectionReferenceNode::Exiled if self.last_exiled.is_some() => {
                return Ok(self.last_exiled.clone().expect("checked exile result"));
            }
            CardCollectionReferenceNode::Revealed => {
                return self
                    .last_revealed
                    .clone()
                    .ok_or_else(|| cursor.fail("a preceding reveal action of this program"));
            }
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
            CardCollectionReferenceNode::Exiled
            | CardCollectionReferenceNode::Linked
            | CardCollectionReferenceNode::Revealed => {
                unreachable!("linked, exiled and revealed references are resolved above")
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
        for (result, _) in [&mut self.last_revealed, &mut self.last_exiled]
            .into_iter()
            .flatten()
        {
            replace_export_references(result, &exports);
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
            let objects =
                self.zone_collection_objects(cursor, query, declarations, target_decisions)?;
            if let Some(quantity) = &query.quantity {
                // The acting player chooses on resolution (no target): as many
                // matching cards as stated, or all of them if fewer exist; up
                // to a count, or any number of them.
                let available = count(objects.clone());
                let id = fresh("chosenZoneCards", &mut self.next_selection);
                self.allocated_ids.insert(id.clone());
                let mut choose = json!({"kind":"chooseCards", "id":id, "player":controller(),
                    "from":objects.clone()});
                let fixed = match quantity {
                    CardSelectionQuantityNode::Exactly(amount) => {
                        let NumericExpression::Atom(NumericAtom::Integer(fixed)) = amount else {
                            return Err(
                                cursor.fail("a literal number of cards chosen from the zone")
                            );
                        };
                        cursor.require(*fixed > 0, "a positive number of chosen cards")?;
                        choose["count"] =
                            json!({"kind":"minimum", "operands":[integer(*fixed), available]});
                        Some(*fixed)
                    }
                    CardSelectionQuantityNode::UpTo(amount) => {
                        let value = lower_targetless_numeric_expression(amount, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        choose["minimum"] = integer(0);
                        choose["maximum"] =
                            json!({"kind":"minimum", "operands":[value, available]});
                        match amount {
                            NumericExpression::Atom(NumericAtom::Integer(fixed)) => Some(*fixed),
                            _ => None,
                        }
                    }
                    CardSelectionQuantityNode::AnyNumber => {
                        choose["minimum"] = integer(0);
                        choose["maximum"] = available;
                        None
                    }
                    CardSelectionQuantityNode::All { .. } => {
                        return Err(cursor.fail("a chosen quantity of zone cards"));
                    }
                };
                effects.push(choose);
                self.clear_antecedent();
                let result = decision_result(&id);
                self.mention((result.clone(), fixed));
                return Ok((result, fixed));
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
        if let CardSelectionQuantityNode::All { via, except_cast } = &selection.quantity {
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
            // `that weren't cast`: the members the preceding resolution cast
            // of this scope did not cast (CR 608.2c).
            let result = if *except_cast {
                let cast = self.last_cast.clone().ok_or_else(|| {
                    cursor.fail("a preceding resolution-time cast of the restated cards")
                })?;
                difference(result, cast)
            } else {
                result
            };
            self.selected.push(result.clone());
            self.complement_exclusions.push(result.clone());
            // A filter or a subtraction leaves no static size.
            let maximum = if predicate.is_none() && !*except_cast {
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

    /// The player-owned zone a zone collection names, with its filter. A
    /// declared zone owner becomes this scope's player target decision.
    fn zone_collection_objects(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        query: &CardZoneCollectionNode,
        declarations: TargetlessDeclarationSupport,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Value> {
        cursor.require(
            matches!(
                query.zone,
                CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
            ),
            "an executable hand, graveyard or exile collection",
        )?;
        let player = self.zone_owner_player(cursor, &query.owner, target_decisions)?;
        let mut objects =
            json!({"kind":"cardsInZone","zone":{"kind":query.zone.ir_name(),"player":player}});
        if let Some(predicate) = &query.predicate {
            validate_object_predicate_numeric_expressions(predicate, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            objects["where"] = lower_card_object_predicate(predicate)
        }
        Ok(objects)
    }

    /// The player a zone owner names: the controller, or a declared player
    /// target this scope adds as a decision.
    fn zone_owner_player(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        owner: &CardZoneOwnerNode,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Value> {
        match owner {
            CardZoneOwnerNode::Player(LocalPlayerReference::Controller) => Ok(controller()),
            CardZoneOwnerNode::Player(_) => {
                Err(cursor.fail("a resolved collection owner reference"))
            }
            CardZoneOwnerNode::TargetPlayer(targets) => {
                let [slot] = targets.slots.as_slice() else {
                    return Err(cursor.fail("one zone-owner target slot"));
                };
                let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                    .ok_or_else(|| cursor.fail("a player target selector for the zone owner"))?;
                cursor.require(
                    matches!(slot.selector, TargetSelector::Player(_))
                        && slot.cardinality == TargetCardinality::EXACTLY_ONE,
                    "a singular targeted player as zone owner",
                )?;
                let id = fresh("targetZonePlayer", &mut self.next_zone_target);
                self.allocated_ids.insert(id.clone());
                target_decisions.push(target_decision(&id, candidates, 1, 1));
                Ok(chosen_target(&id))
            }
        }
    }

    /// The zone the observed cards a reference names are in, for a cast
    /// from among them. Linked exiles are in exile (CR 607.2a). A card in
    /// another player's hand is cast by its caster (CR 601.2a).
    fn observed_cast_zone(
        &self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<CardZoneName> {
        let linked_exile = reference == CardCollectionReferenceNode::Linked
            || (reference == CardCollectionReferenceNode::Exiled
                && (self.last_exiled.is_some()
                    || self.view.is_none()
                    || self.view_verb != Some(CardCollectionVerb::Exiled)));
        if linked_exile {
            return Ok(CardZoneName::Exile);
        }
        let zone = self.view_zone.ok_or_else(|| {
            cursor.fail("observed cards whose zone (hand, graveyard or exile) is proved")
        })?;
        Ok(zone)
    }

    /// CR 608.2g: `you may cast N of them|SPELLS from ORIGIN without paying`
    /// casts during this resolution, from the zone the cards are proved to
    /// be in. The cast cards are bound for a later `that weren't cast`.
    fn lower_resolution_cast(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        permit: &CardPermissionNode,
        declarations: TargetlessDeclarationSupport,
        effects: &mut Vec<Value>,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.enter("lower.card-action.resolution-cast");
        cursor.require(
            permit.verb == CardPermissionVerb::Cast,
            "`cast`: playing a land is not a resolution-time free cast",
        )?;
        cursor.require(
            !permit.mana_of_any_type,
            "a spending policy only on a later casting permission",
        )?;
        fn cast_maximum(
            cursor: &OracleTokenCursor<'_>,
            quantity: &CardSelectionQuantityNode,
        ) -> MachineResult<Option<i64>> {
            match quantity {
                CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                    NumericAtom::Integer(value),
                ))
                | CardSelectionQuantityNode::UpTo(NumericExpression::Atom(NumericAtom::Integer(
                    value,
                ))) if *value > 0 => Ok(Some(*value)),
                CardSelectionQuantityNode::AnyNumber
                | CardSelectionQuantityNode::All {
                    via: None,
                    except_cast: false,
                } => Ok(None),
                _ => Err(cursor
                    .fail("a positive literal, `any number` or every one of the cards to cast")),
            }
        }
        let (cards, maximum, zone) = match &permit.cards {
            CardOperandNode::Reference(reference) => {
                let (cards, maximum) = self.reference(cursor, *reference)?;
                (cards, maximum, self.observed_cast_zone(cursor, *reference)?)
            }
            CardOperandNode::Selection(selection) => {
                let (from, _) = self.reference(cursor, selection.from)?;
                let maximum = cast_maximum(cursor, &selection.quantity)?;
                let cards = match &selection.predicate {
                    Some(predicate) => {
                        validate_object_predicate_numeric_expressions(predicate, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        json!({"kind":"filterObjects", "objects":from,
                            "where":lower_card_object_predicate(predicate)})
                    }
                    None => from,
                };
                (
                    cards,
                    maximum,
                    self.observed_cast_zone(cursor, selection.from)?,
                )
            }
            CardOperandNode::ZoneCollection(query) => {
                let cards =
                    self.zone_collection_objects(cursor, query, declarations, target_decisions)?;
                let maximum = match &query.quantity {
                    Some(quantity) => cast_maximum(cursor, quantity)?,
                    None => None,
                };
                (cards, maximum, query.zone)
            }
            // `you may cast target instant card from your graveyard without
            // paying`: the declared cards, cast from their declared zone.
            CardOperandNode::Targets(target) => {
                let (cards, maximum) = self.operand(
                    cursor,
                    &permit.cards,
                    declarations,
                    effects,
                    target_decisions,
                )?;
                (cards, maximum, target.zone)
            }
            _ => return Err(cursor.fail("an observed card collection to cast from")),
        };
        cursor.require(
            matches!(
                zone,
                CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
            ),
            "a hand, graveyard or exile source zone for the cast",
        )?;
        let id = fresh("castCards", &mut self.next_cast);
        self.allocated_ids.insert(id.clone());
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
            "bind": id,
        });
        if let Some(maximum) = maximum {
            cast["maximum"] = integer(maximum);
        }
        effects.push(cast);
        self.last_cast = Some(bound(&id));
        Ok(std::mem::take(effects))
    }

    /// The card reference a casting permission names: a bound collection or
    /// decision result directly, any other collection value through a new
    /// binding fixed while the effect resolves (CR 608.2c).
    fn permission_card_reference(
        &mut self,
        cards: Value,
        maximum: Option<i64>,
        effects: &mut Vec<Value>,
    ) -> Value {
        match value_kind_name(&cards) {
            Some("boundObjects") if maximum.is_some_and(|maximum| maximum <= 1) => {
                json!({"kind":"boundObject", "binding":cards["binding"]})
            }
            Some("boundObjects" | "decisionResult") => cards,
            _ => {
                let id = fresh("permittedCards", &mut self.next_permission);
                self.allocated_ids.insert(id.clone());
                effects.push(json!({"kind":"bind", "id":id, "value":cards}));
                bound(&id)
            }
        }
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
            CardActionNode::LookAt(operand) => {
                cursor.enter("lower.card-action.look-at");
                match operand {
                    // `look at the top N cards of target player's library`:
                    // a private look by the controller (CR 401.2).
                    CardOperandNode::LibraryTop(top) => {
                        let CardLibraryOwnerNode::TargetPlayer(targets) = &top.owner else {
                            return Err(cursor.fail("a declared library owner"));
                        };
                        let player = self.zone_owner_player(
                            cursor,
                            &CardZoneOwnerNode::TargetPlayer(targets.clone()),
                            target_decisions,
                        )?;
                        let amount_ir =
                            lower_targetless_numeric_expression(&top.count, declarations)
                                .map_err(|error| cursor.fail(error.expectation()))?;
                        let maximum = match &top.count {
                            NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                            _ => None,
                        };
                        let id = fresh("lookedCards", &mut self.next_view);
                        self.allocated_ids.insert(id.clone());
                        self.observe(&id, maximum);
                        effects.push(json!({"kind":"lookAtTopCards",
                            "zone":{"kind":"library", "player":player},
                            "count":amount_ir, "bind":id}));
                    }
                    // `look at target player's hand`: the controller sees the
                    // whole hand; `from it` names that hand afterwards.
                    CardOperandNode::ZoneCollection(query)
                        if query.zone == CardZoneName::Hand
                            && query.predicate.is_none()
                            && query.quantity.is_none() =>
                    {
                        let player =
                            self.zone_owner_player(cursor, &query.owner, target_decisions)?;
                        let id = fresh("lookedHand", &mut self.next_view);
                        self.allocated_ids.insert(id.clone());
                        self.observe(&id, None);
                        self.view_zone = Some(CardZoneName::Hand);
                        self.view_zone_noun = true;
                        effects.push(json!({"kind":"lookAtHand",
                            "player":player.clone(), "viewer":controller()}));
                        effects.push(json!({"kind":"bind", "id":id,
                            "value":{"kind":"cardsInZone",
                                "zone":{"kind":"hand", "player":player}}}));
                    }
                    _ => return Err(cursor.fail("a library top or a whole hand to look at")),
                }
            }
            CardActionNode::Reveal(operand) => {
                let (cards, maximum) = self.operand(
                    cursor,
                    operand,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                effects.push(json!({"kind":"revealCards", "cards":cards.clone()}));
                if matches!(operand, CardOperandNode::LibraryTop(_)) {
                    self.view_verb = Some(CardCollectionVerb::Revealed);
                }
                self.last_revealed = Some((cards, maximum));
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
                if permit.without_paying_mana_cost
                    && permit.duration.is_none()
                    && permit.window.is_none() =>
            {
                return self.lower_resolution_cast(
                    cursor,
                    permit,
                    declarations,
                    &mut effects,
                    target_decisions,
                );
            }
            CardActionNode::Permit(permit) => {
                // `you may cast creature spells from your graveyard this turn`:
                // a permission over a zone's matching cards, current and
                // future, not over cards fixed when the effect resolved.
                if let CardOperandNode::ZoneCollection(query) = &permit.cards {
                    cursor.enter("lower.card-action.permit.zone");
                    cursor.require(
                        permit.verb == CardPermissionVerb::Cast
                            && query.quantity.is_none()
                            && query.owner
                                == CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
                            && matches!(query.zone, CardZoneName::Graveyard | CardZoneName::Exile)
                            && permit.duration == Some(EffectDurationNode::UntilEndOfCurrentTurn)
                            && permit.window.is_none()
                            && !permit.without_paying_mana_cost
                            && !permit.mana_of_any_type,
                        "every matching spell of your graveyard or exile, cast this turn at its usual cost",
                    )?;
                    let mut filter = json!({"kind":"canBeCastAsSpell"});
                    if let Some(predicate) = &query.predicate {
                        validate_object_predicate_numeric_expressions(predicate, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        filter = lower_card_object_predicate(predicate);
                    }
                    effects.push(json!({"kind":"grantZoneCastingUntilEndOfTurn",
                        "player":controller(), "sourceZone":query.zone.ir_name(),
                        "where":filter}));
                    return Ok(effects);
                }
                let (cards, maximum) = self.operand(
                    cursor,
                    &permit.cards,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                cursor.enter("lower.card-action.permit");
                // Cards this program exiled, or cards exiled with the source
                // (CR 607.2a): both are in exile when the permission begins.
                let permits_exiled_view = (self.view_verb == Some(CardCollectionVerb::Exiled)
                    && Some(&cards) == self.view.as_ref())
                    || value_kind_name(&cards) == Some("cardsExiledWithSource");
                cursor.require(
                    matches!(
                        value_kind_name(&cards),
                        Some(
                            "boundObjects"
                                | "decisionResult"
                                | "chosenTargets"
                                | "filterObjects"
                                | "setDifference"
                                | "cardsExiledWithSource"
                        )
                    ),
                    "a bound card collection as the permission object",
                )?;
                let card = self.permission_card_reference(cards, maximum, &mut effects);
                let mut replace_from_source = false;
                let duration = match (&permit.duration, permit.window) {
                    // CR 611.2a + 400.7: an unbounded permission over cards
                    // this program exiled lasts while they remain in exile.
                    (None, None) => {
                        cursor.enter("lower.card-action.permit.unbounded");
                        cursor.require(
                            permits_exiled_view,
                            "an unbounded permission over the cards this program exiled",
                        )?;
                        json!({"kind":"whileInExile"})
                    }
                    (None, Some(CardPermissionWindowNode::WhileInExile)) => {
                        json!({"kind":"whileInExile"})
                    }
                    (None, Some(CardPermissionWindowNode::UntilYourNextEndStep)) => {
                        json!({"kind":"untilNextEndStep"})
                    }
                    // The next card this source exiles replaces the window;
                    // until then the permission lasts while its card is in
                    // exile (CR 400.7).
                    (None, Some(CardPermissionWindowNode::UntilSourceExilesAnother)) => {
                        replace_from_source = true;
                        json!({"kind":"whileInExile"})
                    }
                    (Some(EffectDurationNode::UntilEndOfCurrentTurn), None) => {
                        json!({"kind":"untilEndOfCurrentTurn"})
                    }
                    (
                        Some(EffectDurationNode::UntilEndOfNextTurn {
                            player: LocalPlayerReference::Controller,
                        }),
                        None,
                    ) => json!({"kind":"untilEndOfNextTurn", "player":controller()}),
                    (Some(EffectDurationNode::WhileSourceControlled), None) => {
                        json!({"kind":"whileSourceControlled"})
                    }
                    _ => {
                        return Err(cursor.engine_gap(
                            crate::oracle::canonical::engine_gap::EngineGap::PermissionWindow,
                            "a permission window the runtime can end: this turn, the end of your next turn, your next end step, exile, or source control",
                        ));
                    }
                };
                // `without paying its mana cost` (CR 118.9): the permitted
                // cast has no mana cost to pay.
                let normal_costs_apply = !permit.without_paying_mana_cost;
                let mut action = json!({"kind":match permit.verb {
                        CardPermissionVerb::Play => "play",
                        CardPermissionVerb::Cast => "cast",
                    },
                    "card":card, "normalTimingApplies":true,
                    "normalCostsApply":normal_costs_apply});
                if permit.mana_of_any_type {
                    action["spendManaAsAnyType"] = json!(true);
                }
                let mut grant = json!({"kind":"grantPermission", "player":controller(),
                    "action":action, "duration":duration});
                if replace_from_source {
                    grant["replacePermissionsFromSource"] = json!(true);
                }
                effects.push(grant);
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
                    // The exiled library cards are the observed collection,
                    // now in exile (CR 400.7: their new objects).
                    self.view_verb = Some(CardCollectionVerb::Exiled);
                    self.view_zone = Some(CardZoneName::Exile);
                }
                // `the exiled card(s)` later names these cards, when their
                // identities are fixed (never a zone query re-read after
                // the move empties it).
                if destination.zone == CardZoneName::Exile
                    && matches!(
                        value_kind_name(&cards),
                        Some(
                            "boundObjects"
                                | "decisionResult"
                                | "chosenTargets"
                                | "filterObjects"
                                | "setDifference"
                        )
                    )
                {
                    self.last_exiled = Some((cards.clone(), maximum));
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
