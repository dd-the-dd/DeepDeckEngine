use super::super::model::{AbilitySource, RecognizedEntity};
use super::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug)]
pub(crate) struct AbilityInput<'a> {
    pub face_name: &'a str,
    pub face_type_line: &'a str,
    pub source: &'a AbilitySource,
    pub tokens: &'a [RecognizedEntity],
    pub name_links: &'a [ResolvedNameLink],
    pub name_resolution_failure: Option<&'a NameResolutionFailure>,
}
