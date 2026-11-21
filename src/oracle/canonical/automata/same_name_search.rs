use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectKind, ObjectPredicate, recognize_object_query};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SameNameSearchQuantity {
    AnyNumber,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedSameNameSearchNode {
    pub excluded_target: ObjectPredicate,
    pub quantity: SameNameSearchQuantity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetedSameNameSearchState {
    Choose,
    Target,
    TargetDomain,
    TargetZone,
    Exclusion,
    ExclusionArticle,
    ExclusionPredicate,
    ExclusionCardNoun,
    TargetTerminator,
    Search,
    OwnerZones,
    Quantity,
    Cards,
    NameRelation,
    TargetReference,
    ExileConnector,
    ExileAction,
    ExileReference,
    SearchTerminator,
    Sequence,
    PlayerReference,
    Shuffle,
    FinalTerminator,
    Complete,
}

fn contains_card_domain(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Card) => true,
        ObjectPredicate::Intersection(operands) => operands.iter().any(contains_card_domain),
        _ => false,
    }
}

/// Recognizes a target-name-linked search across the target owner's hidden
/// and public zones. Target exclusion, quantity and anaphoric references are
/// retained independently for lowering.
pub(in crate::oracle::canonical) fn recognize_targeted_same_name_search(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetedSameNameSearchNode> {
    let mut state = TargetedSameNameSearchState::Choose;
    let mut excluded_target = None;
    let mut quantity = None;

    loop {
        state = match state {
            TargetedSameNameSearchState::Choose => {
                cursor.enter("same-name-search.choose");
                cursor.consume("choose")?;
                TargetedSameNameSearchState::Target
            }
            TargetedSameNameSearchState::Target => {
                cursor.enter("same-name-search.target");
                cursor.consume("target")?;
                TargetedSameNameSearchState::TargetDomain
            }
            TargetedSameNameSearchState::TargetDomain => {
                cursor.enter("same-name-search.target-domain");
                cursor.consume("card")?;
                TargetedSameNameSearchState::TargetZone
            }
            TargetedSameNameSearchState::TargetZone => {
                cursor.enter("same-name-search.target-zone");
                cursor.consume_phrase(&["in", "a", "graveyard"])?;
                TargetedSameNameSearchState::Exclusion
            }
            TargetedSameNameSearchState::Exclusion => {
                cursor.enter("same-name-search.exclusion");
                cursor.consume_phrase(&["other", "than"])?;
                TargetedSameNameSearchState::ExclusionArticle
            }
            TargetedSameNameSearchState::ExclusionArticle => {
                cursor.enter("same-name-search.exclusion-article");
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                }) {
                    cursor.advance_to(cursor.position() + 1)?;
                }
                TargetedSameNameSearchState::ExclusionPredicate
            }
            TargetedSameNameSearchState::ExclusionPredicate => {
                cursor.enter("same-name-search.exclusion-predicate");
                let query = recognize_object_query(cursor)?;
                cursor.require(query.controller.is_none(), "an unscoped card predicate")?;
                cursor.require(
                    contains_card_domain(&query.predicate),
                    "a predicate ending in the card domain",
                )?;
                excluded_target = Some(query.predicate);
                TargetedSameNameSearchState::ExclusionCardNoun
            }
            TargetedSameNameSearchState::ExclusionCardNoun => {
                // `recognize_object_query` consumes the `card` domain itself.
                cursor.enter("same-name-search.exclusion-card-domain-complete");
                TargetedSameNameSearchState::TargetTerminator
            }
            TargetedSameNameSearchState::TargetTerminator => {
                cursor.enter("same-name-search.target-terminator");
                cursor.consume(".")?;
                TargetedSameNameSearchState::Search
            }
            TargetedSameNameSearchState::Search => {
                cursor.enter("same-name-search.search");
                cursor.consume("search")?;
                TargetedSameNameSearchState::OwnerZones
            }
            TargetedSameNameSearchState::OwnerZones => {
                cursor.enter("same-name-search.owner-zones");
                cursor.consume("its")?;
                let owner = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("the possessive owner reference"))?
                    .to_string();
                cursor.require(
                    owner.eq_ignore_ascii_case("owner's") || owner.eq_ignore_ascii_case("owner’s"),
                    "the possessive owner reference",
                )?;
                cursor.consume(&owner)?;
                cursor.consume_phrase(&["graveyard", ",", "hand", ",", "and", "library", "for"])?;
                TargetedSameNameSearchState::Quantity
            }
            TargetedSameNameSearchState::Quantity => {
                cursor.enter("same-name-search.quantity");
                quantity = Some(
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
                    {
                        cursor.consume("all")?;
                        SameNameSearchQuantity::All
                    } else {
                        cursor.consume_phrase(&["any", "number", "of"])?;
                        SameNameSearchQuantity::AnyNumber
                    },
                );
                TargetedSameNameSearchState::Cards
            }
            TargetedSameNameSearchState::Cards => {
                cursor.enter("same-name-search.cards");
                cursor.consume("cards")?;
                TargetedSameNameSearchState::NameRelation
            }
            TargetedSameNameSearchState::NameRelation => {
                cursor.enter("same-name-search.name-relation");
                cursor.consume_phrase(&["with", "the", "same", "name", "as"])?;
                TargetedSameNameSearchState::TargetReference
            }
            TargetedSameNameSearchState::TargetReference => {
                cursor.enter("same-name-search.target-reference");
                cursor.consume_phrase(&["that", "card"])?;
                TargetedSameNameSearchState::ExileConnector
            }
            TargetedSameNameSearchState::ExileConnector => {
                cursor.enter("same-name-search.exile-connector");
                cursor.consume("and")?;
                TargetedSameNameSearchState::ExileAction
            }
            TargetedSameNameSearchState::ExileAction => {
                cursor.enter("same-name-search.exile-action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Exile,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                TargetedSameNameSearchState::ExileReference
            }
            TargetedSameNameSearchState::ExileReference => {
                cursor.enter("same-name-search.exile-reference");
                cursor.consume("them")?;
                TargetedSameNameSearchState::SearchTerminator
            }
            TargetedSameNameSearchState::SearchTerminator => {
                cursor.enter("same-name-search.search-terminator");
                cursor.consume(".")?;
                TargetedSameNameSearchState::Sequence
            }
            TargetedSameNameSearchState::Sequence => {
                cursor.enter("same-name-search.sequence");
                cursor.consume("then")?;
                TargetedSameNameSearchState::PlayerReference
            }
            TargetedSameNameSearchState::PlayerReference => {
                cursor.enter("same-name-search.player-reference");
                cursor.consume_phrase(&["that", "player"])?;
                TargetedSameNameSearchState::Shuffle
            }
            TargetedSameNameSearchState::Shuffle => {
                cursor.enter("same-name-search.shuffle");
                cursor.consume("shuffles")?;
                TargetedSameNameSearchState::FinalTerminator
            }
            TargetedSameNameSearchState::FinalTerminator => {
                cursor.enter("same-name-search.final-terminator");
                cursor.consume(".")?;
                TargetedSameNameSearchState::Complete
            }
            TargetedSameNameSearchState::Complete => {
                cursor.enter("same-name-search.complete");
                cursor.finish()?;
                return Ok(TargetedSameNameSearchNode {
                    excluded_target: cursor
                        .require_value(excluded_target, "a target exclusion predicate")?,
                    quantity: cursor.require_value(quantity, "a search quantity")?,
                });
            }
        };
    }
}
