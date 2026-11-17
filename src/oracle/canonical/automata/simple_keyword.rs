use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    KeywordAtomNode, KeywordListNode, KeywordListRecognition, try_recognize_standalone_keyword_list,
};

pub(in crate::oracle::canonical) enum StandaloneKeywordRecognition {
    Parsed(KeywordListNode),
    FailedParameterizedResource(MachineFailure),
    Failed(MachineFailure),
    RejectedEnvelope {
        failure: MachineFailure,
        parameterized_resource: bool,
    },
}

/// The ordinary wrapper preserves every existing parent/caller's Result API.
pub(in crate::oracle::canonical) fn recognize_standalone_keyword_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KeywordListNode> {
    match try_recognize_standalone_keyword_ability(cursor) {
        StandaloneKeywordRecognition::Parsed(list) => Ok(list),
        StandaloneKeywordRecognition::FailedParameterizedResource(failure)
        | StandaloneKeywordRecognition::Failed(failure)
        | StandaloneKeywordRecognition::RejectedEnvelope { failure, .. } => Err(failure),
    }
}

pub(in crate::oracle::canonical) fn try_recognize_standalone_keyword_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> StandaloneKeywordRecognition {
    cursor.enter("keyword.standalone.list");
    let list = match try_recognize_standalone_keyword_list(cursor) {
        KeywordListRecognition::Parsed(list) => list,
        KeywordListRecognition::FailedParameterizedResource(failure) => {
            return StandaloneKeywordRecognition::FailedParameterizedResource(failure);
        }
        KeywordListRecognition::Failed(failure) => {
            return StandaloneKeywordRecognition::Failed(failure);
        }
    };
    let parameterized_resource = list
        .keywords
        .iter()
        .any(|atom| matches!(atom, KeywordAtomNode::Ward { .. }));
    if cursor.peek() == Some(".") {
        if let Err(failure) = cursor.consume(".") {
            return StandaloneKeywordRecognition::RejectedEnvelope {
                failure,
                parameterized_resource,
            };
        }
    }
    cursor.enter("keyword.standalone.complete");
    if let Err(failure) = cursor.finish() {
        return StandaloneKeywordRecognition::RejectedEnvelope {
            failure,
            parameterized_resource,
        };
    }
    StandaloneKeywordRecognition::Parsed(list)
}

#[cfg(test)]
#[path = "simple_keyword_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keyword_typed_commit_tests.rs"]
mod typed_commit_tests;

#[cfg(test)]
#[path = "changeling_keyword_tests.rs"]
mod changeling_tests;

#[cfg(test)]
#[path = "keyword_reminder_registry_tests.rs"]
mod reminder_registry_tests;

#[cfg(test)]
#[path = "landwalk_keyword_tests.rs"]
mod landwalk_tests;
