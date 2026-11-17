//! Lexical card-collection bindings shared by ordinary action sequences.
use super::super::*;

#[derive(Clone, Default)]
pub(in crate::oracle::canonical) struct CardCollectionMemory {
    view: Option<Value>,
    view_maximum: Option<i64>,
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

fn fresh(base: &str, next: &mut usize) -> String {
    *next += 1;
    if *next == 1 {
        base.to_owned()
    } else {
        format!("{base}{next}")
    }
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
        self.singular_focus = None;
        self.plural_focus = None;
        self.removed_up_to_size = 0;
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
            if target.owner == CardTargetZoneOwner::OnePlayer
                && target.cardinality != ExpressedTargetCardinality::EXACTLY_ONE
            {
                decision["selectionConstraint"] =
                    json!({"kind":"sameZoneOwner","zone":target.zone.ir_name()});
            }
            target_decisions.push(decision);
            self.clear_antecedent();
            let maximum = match target.cardinality.maximum {
                NumericAtom::Integer(value) => Some(value),
                _ => None,
            };
            return Ok((json!({"kind":"chosenTargets","id":id}), maximum));
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
        let predicate = selection
            .predicate
            .as_ref()
            .map(lower_card_object_predicate);
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
                self.clear_antecedent();
                self.view = Some(bound(&id));
                self.view_maximum = match amount {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                    _ => None,
                };
                self.complement_basis = Some((bound(&id), self.view_maximum));
                self.mention((bound(&id), self.view_maximum));
                // A one-card observation can still be referenced as a collection.
                self.plural_focus = Some((bound(&id), self.view_maximum));
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
                    CardMoveOwnerNode::EachCardOwner => {
                        cursor.require(
                            destination.zone == CardZoneName::Exile,
                            "a per-card owner destination supported by this movement boundary",
                        )?;
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
                // Battlefield movement still requires admitted object identities
                // and entry replacement proofs; do not claim that backend here.
                cursor.require(
                    destination.zone != CardZoneName::Battlefield && !destination.tapped,
                    "a nonbattlefield destination in this card-collection lowering boundary",
                )?;
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
