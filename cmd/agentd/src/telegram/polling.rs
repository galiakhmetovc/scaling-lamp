use teloxide::types::Update;

#[cfg_attr(not(test), allow(dead_code))]
pub fn next_confirmed_offset(updates: &[Update]) -> Option<i32> {
    updates
        .iter()
        .map(|update| update.id.0.saturating_add(1))
        .max()
        .and_then(|value| i32::try_from(value).ok())
}
