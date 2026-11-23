use super::super::{MachineFailure, OracleTokenCursor};
use super::{
    KeywordAtomNode, ObjectQueryNode, recognize_keyword_atom_reminder_annotation,
    recognize_object_collection, recognize_standalone_keyword_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StaticCollectionKeywordGrantNode {
    pub objects: ObjectQueryNode,
    pub keywords: Vec<KeywordAtomNode>,
    pub exclude_source: bool,
}

pub(in crate::oracle::canonical) enum StaticCollectionKeywordRecognition {
    Parsed(StaticCollectionKeywordGrantNode),
    NotClaimed,
    UnsupportedEnvelope(MachineFailure),
    Failed(MachineFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticCollectionKeywordState {
    Objects,
    Predicate,
    Keywords,
    Terminator,
    Annotation,
    Complete,
}

#[derive(Default)]
struct StaticCollectionKeywordFrame {
    objects: Option<ObjectQueryNode>,
    keywords: Vec<KeywordAtomNode>,
    exclude_source: bool,
    quoted: bool,
}

/// One persistent collection predicate, not a temporary effect or a condition.
/// The shared collection and keyword children own their own Boolean/list
/// connectors. Only a completely consumed envelope commits this static node.
pub(in crate::oracle::canonical) fn try_recognize_static_collection_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
) -> StaticCollectionKeywordRecognition {
    let checkpoint = cursor.checkpoint();
    let mut state = StaticCollectionKeywordState::Objects;
    let mut frame = StaticCollectionKeywordFrame::default();

    loop {
        state = match state {
            StaticCollectionKeywordState::Objects => {
                cursor.enter("static.collection-keyword.objects");
                let objects = match recognize_object_collection(cursor) {
                    Ok(objects) => objects,
                    Err(_) => {
                        cursor.rollback(checkpoint);
                        return StaticCollectionKeywordRecognition::NotClaimed;
                    }
                };
                frame.exclude_source = objects.exclude_source;
                frame.objects = Some(objects);
                StaticCollectionKeywordState::Predicate
            }
            StaticCollectionKeywordState::Predicate => {
                cursor.enter("static.collection-keyword.predicate");
                let Some(predicate) = cursor.peek().filter(|raw| {
                    raw.eq_ignore_ascii_case("have") || raw.eq_ignore_ascii_case("has")
                }) else {
                    cursor.rollback(checkpoint);
                    return StaticCollectionKeywordRecognition::NotClaimed;
                };
                let predicate = predicate.to_string();
                if let Err(failure) = cursor.consume(&predicate) {
                    return StaticCollectionKeywordRecognition::Failed(failure);
                }
                let Some(objects) = frame.objects.as_ref() else {
                    return StaticCollectionKeywordRecognition::Failed(
                        cursor.fail("a populated collection subject"),
                    );
                };
                if let Err(failure) = cursor.require(
                    objects.denotes_battlefield_permanents(),
                    "a collection explicitly anchored in the battlefield-permanent domain",
                ) {
                    return StaticCollectionKeywordRecognition::Failed(failure);
                }
                StaticCollectionKeywordState::Keywords
            }
            StaticCollectionKeywordState::Keywords => {
                cursor.enter("static.collection-keyword.keywords");
                frame.quoted = cursor.peek() == Some("\"");
                if frame.quoted {
                    if let Err(failure) = cursor.consume("\"") {
                        return StaticCollectionKeywordRecognition::Failed(failure);
                    }
                }
                frame.keywords = match recognize_standalone_keyword_list(cursor) {
                    Ok(keywords) => keywords.keywords,
                    Err(failure) => return StaticCollectionKeywordRecognition::Failed(failure),
                };
                StaticCollectionKeywordState::Terminator
            }
            StaticCollectionKeywordState::Terminator => {
                cursor.enter("static.collection-keyword.terminator");
                let inner_period = frame.quoted && cursor.peek() == Some(".");
                if inner_period {
                    if let Err(failure) = cursor.consume(".") {
                        return StaticCollectionKeywordRecognition::Failed(failure);
                    }
                }
                if frame.quoted {
                    if let Err(failure) = cursor.consume("\"") {
                        return StaticCollectionKeywordRecognition::Failed(failure);
                    }
                }
                if !inner_period || cursor.peek() == Some(".") {
                    if let Err(failure) = cursor.consume(".") {
                        cursor.rollback(checkpoint);
                        return StaticCollectionKeywordRecognition::UnsupportedEnvelope(failure);
                    }
                }
                StaticCollectionKeywordState::Annotation
            }
            StaticCollectionKeywordState::Annotation => {
                cursor.enter("static.collection-keyword.annotation");
                let Some(objects) = frame.objects.as_ref() else {
                    return StaticCollectionKeywordRecognition::Failed(
                        cursor.fail("a bound collection for the reminder"),
                    );
                };
                if let Err(failure) = recognize_keyword_atom_reminder_annotation(
                    cursor,
                    &frame.keywords,
                    &objects.predicate,
                ) {
                    return StaticCollectionKeywordRecognition::Failed(failure);
                }
                StaticCollectionKeywordState::Complete
            }
            StaticCollectionKeywordState::Complete => {
                cursor.enter("static.collection-keyword.complete");
                if let Err(failure) = cursor.finish() {
                    cursor.rollback(checkpoint);
                    return StaticCollectionKeywordRecognition::UnsupportedEnvelope(failure);
                }
                let Some(objects) = frame.objects else {
                    return StaticCollectionKeywordRecognition::Failed(
                        cursor.fail("a populated collection subject"),
                    );
                };
                return StaticCollectionKeywordRecognition::Parsed(
                    StaticCollectionKeywordGrantNode {
                        objects,
                        keywords: frame.keywords,
                        exclude_source: frame.exclude_source,
                    },
                );
            }
        };
    }
}

#[cfg(test)]
#[path = "static_collection_keyword_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "static_collection_keyword_annotation_tests.rs"]
mod annotation_tests;
