use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_owner_library_reveal_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let subject_shape = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("the"))
        && tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("owner") || token.raw.eq_ignore_ascii_case("controller")
        })
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("of"))
        && tokens
            .get(3)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"));
    let owner_library_move = tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("shuffles")
            && window[1].raw.eq_ignore_ascii_case("it")
            && window[2].raw.eq_ignore_ascii_case("into")
    });
    let reveal_sequence = tokens.windows(3).any(|window| {
        window[0].raw == ","
            && window[1].raw.eq_ignore_ascii_case("then")
            && is_verb_form(
                &window[2].raw,
                VerbLemma::Reveal,
                VerbForm::ThirdPersonSingular,
            )
    });
    let conditional_move = tokens
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("if"))
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("onto")
                && window[1].raw.eq_ignore_ascii_case("the")
                && window[2].raw.eq_ignore_ascii_case("battlefield")
        });
    subject_shape && owner_library_move && reveal_sequence && conditional_move
}

pub(in crate::oracle::canonical) fn parse_promoted_owner_library_reveal(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("owner-library-reveal-sequence", input.tokens);
    let sequence = recognize_owner_library_reveal_sequence(&mut cursor)?;
    let (target_id, candidates, minimum, maximum) = lower_single_target(&sequence.target);
    cursor.enter("owner-library.lowering.target-contract");
    cursor.require(
        target_id == "targetPermanent" && minimum == 1 && maximum == 1,
        "one permanent target binding",
    )?;

    let revealed_filter = match &sequence.revealed_card {
        RevealedCardPredicateNode::PermanentCard => json!({ "kind": "isPermanentCard" }),
        RevealedCardPredicateNode::Predicate(predicate) => lower_card_object_predicate(predicate),
    };
    let target = chosen_target(target_id);
    let target_owner = json!({ "kind": "boundValue", "id": "targetOwner" });
    let revealed_top_card = bound_objects("revealedTopCard");
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    minimum,
                    maximum,
                )],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "targetOwner",
                    "value": {
                        "kind": "ownerOf",
                        "object": target.clone(),
                    },
                },
                {
                    "kind": "putPermanentOnOwnerLibrary",
                    "permanent": target,
                    "position": "top",
                },
                {
                    "kind": "shuffleZone",
                    "zone": library(target_owner.clone()),
                },
                {
                    "kind": "lookAtTopCards",
                    "zone": library(target_owner.clone()),
                    "count": integer(1),
                    "bind": "revealedTopCard",
                },
                {
                    "kind": "revealCards",
                    "cards": revealed_top_card.clone(),
                },
                {
                    "kind": "moveCards",
                    "cards": {
                        "kind": "filterObjects",
                        "objects": revealed_top_card,
                        "where": revealed_filter,
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": target_owner,
                        "tapped": false,
                    },
                },
            ],
        }),
        &[
            "Recognize one targeted permanent independently",
            "Bind its owner before the target changes zones",
            "Shuffle and inspect the bound owner's library",
            "Recognize the revealed-card predicate independently",
            "Move only matching revealed cards with primitive effects",
            &trace,
        ],
    ))
}
