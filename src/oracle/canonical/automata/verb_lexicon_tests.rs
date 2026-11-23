//! The lexicon is the only source of verb surfaces: every form of every
//! lemma round-trips, shared surfaces keep all their readings, and words
//! outside the requested lemmas make no claim.
use super::*;

const ALL_FORMS: [VerbForm; 5] = [
    VerbForm::Base,
    VerbForm::ThirdPersonSingular,
    VerbForm::Past,
    VerbForm::PastParticiple,
    VerbForm::PresentParticiple,
];

#[test]
fn every_form_of_every_lemma_round_trips_through_lookup() {
    assert!(ALL_LEMMAS.len() >= 26, "every migrated lemma has an entry");
    for lemma in ALL_LEMMAS.iter().copied() {
        let entry = verb_entry(lemma);
        assert_eq!(entry.lemma, lemma);
        assert!(!entry.frames.is_empty() && !entry.voices.is_empty());
        for form in ALL_FORMS {
            let surface = entry.surface(form);
            let (found, forms) = lookup_verb(&surface.to_ascii_uppercase(), &[lemma])
                .expect("every surface is found case-insensitively");
            assert_eq!(found.lemma, lemma);
            assert!(forms.contains(&form), "{surface}: {forms:?}");
            assert!(is_verb_form(surface, lemma, form));
        }
    }
}

#[test]
fn a_shared_surface_keeps_every_reading_and_agreement_picks_one_form() {
    let (_, forms) = lookup_verb("lost", &[VerbLemma::Lose]).unwrap();
    assert_eq!(forms, [VerbForm::Past, VerbForm::PastParticiple]);
    assert_eq!(VerbAgreement::Base.form(), VerbForm::Base);
    assert_eq!(
        VerbAgreement::ThirdPersonSingular.form(),
        VerbForm::ThirdPersonSingular
    );
    assert_eq!(
        verb_entry(VerbLemma::Gain).surface(VerbAgreement::ThirdPersonSingular.form()),
        "gains"
    );
}

#[test]
fn words_outside_the_requested_lemmas_make_no_claim() {
    assert!(lookup_verb("gains", &[VerbLemma::Lose]).is_none());
    assert!(lookup_verb("gainz", &[VerbLemma::Gain, VerbLemma::Lose]).is_none());
    assert!(lookup_verb("draws", &[VerbLemma::Gain, VerbLemma::Lose]).is_none());
    assert!(!is_verb_form(
        "gain",
        VerbLemma::Gain,
        VerbForm::ThirdPersonSingular
    ));
    assert_eq!(ArgumentFrame::LifeTotalChange.marker_noun(), Some("life"));
}

#[test]
fn a_lemma_holds_a_set_of_frames_and_its_voices() {
    assert_eq!(
        verb_entry(VerbLemma::Gain).frames,
        [
            ArgumentFrame::LifeTotalChange,
            ArgumentFrame::KeywordList,
            ArgumentFrame::Control
        ]
    );
    assert_eq!(
        verb_entry(VerbLemma::Lose).frames,
        [ArgumentFrame::LifeTotalChange, ArgumentFrame::Abilities]
    );
    let deal = verb_entry(VerbLemma::Deal);
    assert_eq!(deal.frames, [ArgumentFrame::DamageDealt]);
    assert_eq!(deal.voices, [Voice::Active, Voice::Passive]);
    assert_eq!(deal.surface(VerbForm::PastParticiple), "dealt");
    // Every frame but the keyword list has a marker noun to claim on.
    assert_eq!(ArgumentFrame::Control.marker_noun(), Some("control"));
    assert_eq!(ArgumentFrame::KeywordList.marker_noun(), None);
}

#[test]
fn effect_head_lemmas_round_trip_but_are_not_performed() {
    for lemma in EFFECT_LEMMAS.iter().copied() {
        let entry = verb_entry(lemma);
        assert_eq!(entry.lemma, lemma);
        for form in ALL_FORMS {
            let (found, forms) = lookup_effect_verb(entry.surface(form))
                .expect("every effect-head surface is found");
            assert_eq!(found.lemma, lemma, "{}", entry.surface(form));
            assert!(forms.contains(&form));
        }
    }
    // The dispatcher reads `destroy`; the performer's person shift does not
    // until its primitive carries a performer.
    assert!(lookup_effect_verb("destroys").is_some());
    assert!(lookup_verb("destroys", &ALL_LEMMAS).is_none());
    assert!(ALL_LEMMAS.iter().all(|lemma| EFFECT_LEMMAS.contains(lemma)));
}

#[test]
fn auxiliaries_and_the_copula_are_closed_tables_with_agreement() {
    assert_eq!(lookup_auxiliary("may"), Some((Auxiliary::May, None)));
    assert_eq!(
        lookup_auxiliary("doesn\u{2019}t"),
        Some((Auxiliary::DoNot, Some(VerbAgreement::ThirdPersonSingular)))
    );
    assert_eq!(
        lookup_auxiliary("don't"),
        Some((Auxiliary::DoNot, Some(VerbAgreement::Base)))
    );
    assert_eq!(lookup_auxiliary("Can't"), Some((Auxiliary::Cannot, None)));
    assert_eq!(lookup_auxiliary("gains"), None);
    assert_eq!(
        copula_agreement("is"),
        Some(VerbAgreement::ThirdPersonSingular)
    );
    assert_eq!(copula_agreement("Are"), Some(VerbAgreement::Base));
    assert_eq!(copula_agreement("be"), None);
}
