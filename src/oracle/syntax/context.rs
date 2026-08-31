use super::super::model::AbilitySource;

#[derive(Clone, Debug)]
pub(crate) struct AbilityInput<'a> {
    pub face_name: &'a str,
    pub face_type_line: &'a str,
    pub source: &'a AbilitySource,
}
