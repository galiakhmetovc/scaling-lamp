#[derive(Debug, Clone, Copy)]
pub(super) struct EnumLikeFieldRepair {
    field: &'static str,
    allowed_values: &'static [&'static str],
}

pub(super) const KNOWLEDGE_READ_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["excerpt", "full"],
}];

pub(super) const SESSION_READ_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["summary", "timeline", "transcript", "artifacts"],
}];

pub(super) const SESSION_WAIT_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["summary", "timeline", "transcript", "artifacts"],
}];

pub(super) const CONTINUE_LATER_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "delivery_mode",
    allowed_values: &["fresh_session", "existing_session"],
}];

pub(super) const SCHEDULE_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[
    EnumLikeFieldRepair {
        field: "mode",
        allowed_values: &["interval", "after_completion", "once"],
    },
    EnumLikeFieldRepair {
        field: "delivery_mode",
        allowed_values: &["fresh_session", "existing_session"],
    },
];

#[derive(Debug, Clone, Copy)]
pub(super) struct BareStringFieldRepair {
    field: &'static str,
}

pub(super) const BROWSER_OUTPUT_PATH_REPAIRS: &[BareStringFieldRepair] =
    &[BareStringFieldRepair { field: "path" }];

pub(super) const BROWSER_TEXT_STRING_REPAIRS: &[BareStringFieldRepair] =
    &[BareStringFieldRepair { field: "selector" }];

pub(super) const DELIVER_FILE_STRING_REPAIRS: &[BareStringFieldRepair] = &[
    BareStringFieldRepair {
        field: "artifact_id",
    },
    BareStringFieldRepair {
        field: "workspace_path",
    },
    BareStringFieldRepair { field: "file_name" },
    BareStringFieldRepair { field: "caption" },
    BareStringFieldRepair { field: "target" },
];

pub(super) const MEMORY_ADD_STRING_REPAIRS: &[BareStringFieldRepair] = &[
    BareStringFieldRepair { field: "text" },
    BareStringFieldRepair { field: "scope" },
];

pub(super) const MEMORY_SEARCH_STRING_REPAIRS: &[BareStringFieldRepair] = &[
    BareStringFieldRepair { field: "query" },
    BareStringFieldRepair { field: "scope" },
];

pub(super) const MEMORY_LIST_STRING_REPAIRS: &[BareStringFieldRepair] =
    &[BareStringFieldRepair { field: "scope" }];

pub(super) const MEMORY_UPDATE_STRING_REPAIRS: &[BareStringFieldRepair] = &[
    BareStringFieldRepair { field: "memory_id" },
    BareStringFieldRepair { field: "text" },
];

pub(super) const MEMORY_DELETE_STRING_REPAIRS: &[BareStringFieldRepair] =
    &[BareStringFieldRepair { field: "memory_id" }];

pub(super) fn repair_bare_enum_like_values(
    input: &str,
    repairs: &[EnumLikeFieldRepair],
) -> Option<String> {
    fn allowed_values_for_field<'a>(
        repairs: &'a [EnumLikeFieldRepair],
        field: &str,
    ) -> Option<&'a [&'static str]> {
        repairs
            .iter()
            .find(|repair| repair.field == field)
            .map(|repair| repair.allowed_values)
    }

    fn is_enum_token_byte(byte: u8) -> bool {
        byte.is_ascii_lowercase() || byte == b'_'
    }

    let bytes = input.as_bytes();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let key_start = index + 1;
        index += 1;
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key = &input[key_start..index];
        index += 1;

        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(allowed_values) = allowed_values_for_field(repairs, key) else {
            continue;
        };
        if cursor >= bytes.len() || bytes[cursor] == b'"' {
            continue;
        }

        let value_start = cursor;
        while cursor < bytes.len() && is_enum_token_byte(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == value_start {
            continue;
        }

        let token = &input[value_start..cursor];
        if !allowed_values.contains(&token) {
            continue;
        }

        let mut delimiter = cursor;
        while delimiter < bytes.len() && bytes[delimiter].is_ascii_whitespace() {
            delimiter += 1;
        }
        if delimiter < bytes.len() && !matches!(bytes[delimiter], b',' | b'}' | b']') {
            continue;
        }

        replacements.push((value_start, cursor, format!("\"{token}\"")));
    }

    if replacements.is_empty() {
        return None;
    }

    let mut repaired = String::with_capacity(input.len() + replacements.len() * 2);
    let mut cursor = 0usize;
    for (start, end, replacement) in replacements {
        repaired.push_str(&input[cursor..start]);
        repaired.push_str(&replacement);
        cursor = end;
    }
    repaired.push_str(&input[cursor..]);
    Some(repaired)
}

pub(super) fn repair_bare_string_field_values(
    input: &str,
    repairs: &[BareStringFieldRepair],
) -> Option<String> {
    fn should_repair_field(repairs: &[BareStringFieldRepair], field: &str) -> bool {
        repairs.iter().any(|repair| repair.field == field)
    }

    let bytes = input.as_bytes();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let key_start = index + 1;
        index += 1;
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key = &input[key_start..index];
        index += 1;

        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if !should_repair_field(repairs, key) {
            continue;
        }
        if cursor >= bytes.len() || matches!(bytes[cursor], b'"' | b'{' | b'[') {
            continue;
        }

        let value_start = cursor;
        while cursor < bytes.len() && !matches!(bytes[cursor], b',' | b'}' | b']') {
            cursor += 1;
        }
        let value_end = cursor;
        let token = input[value_start..value_end].trim_end();
        if token.is_empty() {
            continue;
        }
        if matches!(token, "true" | "false" | "null") {
            continue;
        }

        let quoted = serde_json::to_string(token).ok()?;
        replacements.push((value_start, value_end, quoted));
    }

    if replacements.is_empty() {
        return None;
    }

    let mut repaired = String::with_capacity(input.len() + replacements.len() * 2);
    let mut cursor = 0usize;
    for (start, end, replacement) in replacements {
        repaired.push_str(&input[cursor..start]);
        repaired.push_str(&replacement);
        cursor = end;
    }
    repaired.push_str(&input[cursor..]);
    Some(repaired)
}
