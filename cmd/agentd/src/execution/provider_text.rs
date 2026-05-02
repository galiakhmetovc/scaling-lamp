pub(super) fn truncate_utf8_bytes(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = 0;
    for (index, _) in value.char_indices() {
        if index > max_bytes {
            break;
        }
        end = index;
    }
    if end == 0 {
        return (String::new(), true);
    }
    (value[..end].to_string(), true)
}

fn utf8_boundary_at_or_after(value: &str, mut offset: usize) -> usize {
    offset = offset.min(value.len());
    while offset < value.len() && !value.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

pub(super) fn utf8_byte_page(
    value: &str,
    offset: Option<usize>,
    max_bytes: Option<usize>,
    default_max_bytes: usize,
    hard_max_bytes: usize,
) -> (String, usize, Option<usize>) {
    let start = utf8_boundary_at_or_after(value, offset.unwrap_or(0));
    let limit = max_bytes
        .unwrap_or(default_max_bytes)
        .clamp(1, hard_max_bytes);
    let (content, _) = truncate_utf8_bytes(&value[start..], limit);
    let end = start + content.len();
    let next_offset = (end < value.len()).then_some(end);
    (content, start, next_offset)
}
