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
    for lemma in [VerbLemma::Gain, VerbLemma::Lose] {
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
    assert_eq!(ArgumentFrame::LifeTotalChange.resource_noun(), "life");
}
