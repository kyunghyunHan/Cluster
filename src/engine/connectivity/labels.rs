use crate::model::NetLabelScope;

pub(in crate::engine) fn merge_key(
    scope: NetLabelScope,
    page: usize,
    normalized_label: &str,
) -> Option<String> {
    match scope {
        NetLabelScope::Local => None,
        NetLabelScope::Page => Some(format!("page:{page}:{normalized_label}")),
        NetLabelScope::Global => Some(format!("global:{normalized_label}")),
    }
}
