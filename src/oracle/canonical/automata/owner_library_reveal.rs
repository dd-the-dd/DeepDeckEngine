use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectKind, ObjectPredicate, TargetCardinality, TargetListNode, TargetSelector,
    positive_object_selector_atom, recognize_target_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RevealedCardPredicateNode {
    PermanentCard,
    Predicate(ObjectPredicate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OwnerLibraryRevealSequenceNode {
    pub target: TargetListNode,
    pub revealed_card: RevealedCardPredicateNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerLibraryRevealState {
    SubjectDeterminer,
    PlayerRole,
    PlayerRelation,
    Target,
    ShuffleAction,
    TargetReference,
    OwnerLibraryDestination,
    SequenceConnector,
    RevealAction,
    TopCardCollection,
    FirstTerminator,
    Conditional,
    ConditionalSubject,
    ConditionalArticle,
    RevealedCardPredicate,
    CardNoun,
    ConditionalSeparator,
    PlayerReference,
    PutAction,
    RevealedCardReference,
    BattlefieldDestination,
    FinalTerminator,
    Complete,
}

#[derive(Default)]
struct OwnerLibraryRevealFrame {
    target: Option<TargetListNode>,
    revealed_card: Option<RevealedCardPredicateNode>,
}

fn predicate_is_valid_permanent_target(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Permanent) => true,
        ObjectPredicate::ObjectKind(ObjectKind::Card | ObjectKind::Spell)
        | ObjectPredicate::ChosenCreatureType => false,
        ObjectPredicate::Not(operand) => predicate_is_valid_permanent_target(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().all(predicate_is_valid_permanent_target)
        }
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::ColorQuantity(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::HasKeyword(_)
        | ObjectPredicate::Status(_)
        | ObjectPredicate::NumericComparison { .. } => true,
    }
}

fn target_is_one_permanent(target: &TargetListNode) -> bool {
    let [slot] = target.slots.as_slice() else {
        return false;
    };
    if slot.cardinality != TargetCardinality::EXACTLY_ONE {
        return false;
    }
    match &slot.selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate_is_valid_permanent_target(predicate)
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => false,
    }
}

fn recognize_revealed_card_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RevealedCardPredicateNode> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a card characteristic"))?
        .to_string();
    let predicate = positive_object_selector_atom(&raw)
        .ok_or_else(|| cursor.fail("a known card type, supertype, or `permanent` category"))?;
    cursor.advance_to(cursor.position() + 1)?;
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Permanent) => {
            Ok(RevealedCardPredicateNode::PermanentCard)
        }
        ObjectPredicate::ObjectKind(ObjectKind::Card | ObjectKind::Spell) => {
            Err(cursor.fail("a card characteristic rather than an object domain"))
        }
        predicate => Ok(RevealedCardPredicateNode::Predicate(predicate)),
    }
}

/// Recognizes an owner-bound library sequence while retaining the original
/// target predicate and the revealed-card predicate as independent children.
/// The owner relation is committed before the target moves zones so lowering
/// can preserve that player through the rest of the sequence.
pub(in crate::oracle::canonical) fn recognize_owner_library_reveal_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OwnerLibraryRevealSequenceNode> {
    let mut state = OwnerLibraryRevealState::SubjectDeterminer;
    let mut frame = OwnerLibraryRevealFrame::default();

    loop {
        state = match state {
            OwnerLibraryRevealState::SubjectDeterminer => {
                cursor.enter("owner-library.subject.determiner");
                cursor.consume("the")?;
                OwnerLibraryRevealState::PlayerRole
            }
            OwnerLibraryRevealState::PlayerRole => {
                cursor.enter("owner-library.subject.player-role");
                cursor.consume("owner")?;
                OwnerLibraryRevealState::PlayerRelation
            }
            OwnerLibraryRevealState::PlayerRelation => {
                cursor.enter("owner-library.subject.relation");
                cursor.consume("of")?;
                OwnerLibraryRevealState::Target
            }
            OwnerLibraryRevealState::Target => {
                cursor.enter("owner-library.target");
                let target = recognize_target_list(cursor)?;
                cursor.require(
                    target_is_one_permanent(&target),
                    "exactly one permanent target",
                )?;
                frame.target = Some(target);
                OwnerLibraryRevealState::ShuffleAction
            }
            OwnerLibraryRevealState::ShuffleAction => {
                cursor.enter("owner-library.shuffle.action");
                cursor.consume("shuffles")?;
                OwnerLibraryRevealState::TargetReference
            }
            OwnerLibraryRevealState::TargetReference => {
                cursor.enter("owner-library.shuffle.target-reference");
                cursor.consume("it")?;
                OwnerLibraryRevealState::OwnerLibraryDestination
            }
            OwnerLibraryRevealState::OwnerLibraryDestination => {
                cursor.enter("owner-library.shuffle.destination");
                cursor.consume_phrase(&["into", "their", "library"])?;
                OwnerLibraryRevealState::SequenceConnector
            }
            OwnerLibraryRevealState::SequenceConnector => {
                cursor.enter("owner-library.sequence.connector");
                cursor.consume_phrase(&[",", "then"])?;
                OwnerLibraryRevealState::RevealAction
            }
            OwnerLibraryRevealState::RevealAction => {
                cursor.enter("owner-library.reveal.action");
                cursor.consume("reveals")?;
                OwnerLibraryRevealState::TopCardCollection
            }
            OwnerLibraryRevealState::TopCardCollection => {
                cursor.enter("owner-library.reveal.collection");
                cursor.consume_phrase(&["the", "top", "card", "of", "their", "library"])?;
                OwnerLibraryRevealState::FirstTerminator
            }
            OwnerLibraryRevealState::FirstTerminator => {
                cursor.enter("owner-library.reveal.terminator");
                cursor.consume(".")?;
                OwnerLibraryRevealState::Conditional
            }
            OwnerLibraryRevealState::Conditional => {
                cursor.enter("owner-library.conditional.if");
                cursor.consume("if")?;
                OwnerLibraryRevealState::ConditionalSubject
            }
            OwnerLibraryRevealState::ConditionalSubject => {
                cursor.enter("owner-library.conditional.subject");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it's"))
                {
                    cursor.consume("it's")?;
                } else {
                    cursor.consume_phrase(&["it", "is"])?;
                }
                OwnerLibraryRevealState::ConditionalArticle
            }
            OwnerLibraryRevealState::ConditionalArticle => {
                cursor.enter("owner-library.conditional.article");
                match cursor.peek() {
                    Some(raw)
                        if raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an") =>
                    {
                        let article = raw.to_string();
                        cursor.consume(&article)?;
                    }
                    _ => return Err(cursor.fail("`a` or `an`")),
                }
                OwnerLibraryRevealState::RevealedCardPredicate
            }
            OwnerLibraryRevealState::RevealedCardPredicate => {
                cursor.enter("owner-library.conditional.card-predicate");
                frame.revealed_card = Some(recognize_revealed_card_predicate(cursor)?);
                OwnerLibraryRevealState::CardNoun
            }
            OwnerLibraryRevealState::CardNoun => {
                cursor.enter("owner-library.conditional.card-noun");
                cursor.consume("card")?;
                OwnerLibraryRevealState::ConditionalSeparator
            }
            OwnerLibraryRevealState::ConditionalSeparator => {
                cursor.enter("owner-library.conditional.separator");
                cursor.consume(",")?;
                OwnerLibraryRevealState::PlayerReference
            }
            OwnerLibraryRevealState::PlayerReference => {
                cursor.enter("owner-library.conditional.player-reference");
                cursor.consume("they")?;
                OwnerLibraryRevealState::PutAction
            }
            OwnerLibraryRevealState::PutAction => {
                cursor.enter("owner-library.conditional.put-action");
                cursor.consume("put")?;
                OwnerLibraryRevealState::RevealedCardReference
            }
            OwnerLibraryRevealState::RevealedCardReference => {
                cursor.enter("owner-library.conditional.card-reference");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
                {
                    cursor.consume("it")?;
                } else {
                    cursor.consume_phrase(&["that", "card"])?;
                }
                OwnerLibraryRevealState::BattlefieldDestination
            }
            OwnerLibraryRevealState::BattlefieldDestination => {
                cursor.enter("owner-library.conditional.destination");
                cursor.consume_phrase(&["onto", "the", "battlefield"])?;
                OwnerLibraryRevealState::FinalTerminator
            }
            OwnerLibraryRevealState::FinalTerminator => {
                cursor.enter("owner-library.conditional.terminator");
                cursor.consume(".")?;
                OwnerLibraryRevealState::Complete
            }
            OwnerLibraryRevealState::Complete => {
                cursor.enter("owner-library.complete");
                cursor.finish()?;
                return Ok(OwnerLibraryRevealSequenceNode {
                    target: cursor.require_value(frame.target, "one permanent target")?,
                    revealed_card: cursor
                        .require_value(frame.revealed_card, "one revealed-card predicate")?,
                });
            }
        };
    }
}
