const REDACTED: &str = "<redacted>";

const SENSITIVE_KEYS: &[&str] = &[
    "access_token",
    "api_key",
    "apikey",
    "authorization",
    "bearer_token",
    "bot_token",
    "cookie",
    "jwt",
    "passwd",
    "password",
    "pwd",
    "refresh_token",
    "secret",
    "session_token",
    "token",
];

pub(crate) fn redact_sensitive_text(value: &str) -> String {
    let value = redact_url_credentials(value);
    let value = redact_cli_user_credentials(value.as_str());
    let value = redact_bare_auth_tokens(value.as_str());
    redact_sensitive_assignments(value.as_str())
}

pub(crate) fn redact_sensitive_option(value: Option<String>) -> Option<String> {
    value.map(|value| redact_sensitive_text(value.as_str()))
}

fn redact_url_credentials(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if starts_with_ascii_case_insensitive(bytes, index, b"://") {
            output.extend_from_slice(b"://");
            index += 3;
            let authority_start = index;
            let authority_end = scan_until_any(bytes, index, b"/?# \t\r\n\"'");
            let authority = &bytes[authority_start..authority_end];
            if let Some(at_offset) = authority.iter().position(|byte| *byte == b'@') {
                let userinfo = &authority[..at_offset];
                if let Some(colon_offset) = userinfo.iter().position(|byte| *byte == b':') {
                    output.extend_from_slice(&userinfo[..=colon_offset]);
                    output.extend_from_slice(REDACTED.as_bytes());
                    output.extend_from_slice(&authority[at_offset..]);
                    index = authority_end;
                    continue;
                }
            }
            output.extend_from_slice(authority);
            index = authority_end;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).unwrap_or_else(|_| value.to_string())
}

fn redact_cli_user_credentials(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let matched = [
            b"-u ".as_slice(),
            b"--user ".as_slice(),
            b"--user=".as_slice(),
        ]
        .iter()
        .find(|marker| starts_with_ascii_case_insensitive(bytes, index, marker))
        .map(|marker| marker.len());
        let Some(marker_len) = matched else {
            output.push(bytes[index]);
            index += 1;
            continue;
        };

        output.extend_from_slice(&bytes[index..index + marker_len]);
        index += marker_len;
        let (quote, credential_start) = consume_optional_quote(bytes, index);
        if let Some(quote) = quote {
            output.push(quote);
        }
        let credential_end = scan_until_shell_delimiter(bytes, credential_start, quote);
        let credential = &bytes[credential_start..credential_end];
        if let Some(colon_offset) = credential.iter().position(|byte| *byte == b':') {
            output.extend_from_slice(&credential[..=colon_offset]);
            output.extend_from_slice(REDACTED.as_bytes());
        } else {
            output.extend_from_slice(credential);
        }
        index = credential_end;
        if let Some(quote) = quote
            && index < bytes.len()
            && bytes[index] == quote
        {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| value.to_string())
}

fn redact_bare_auth_tokens(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let marker = if starts_with_ascii_case_insensitive(bytes, index, b"Bearer ") {
            Some(b"Bearer ".as_slice())
        } else if starts_with_ascii_case_insensitive(bytes, index, b"Basic ") {
            Some(b"Basic ".as_slice())
        } else {
            None
        };
        let Some(marker) = marker else {
            output.push(bytes[index]);
            index += 1;
            continue;
        };
        output.extend_from_slice(marker);
        index += marker.len();
        let token_end = scan_until_any(bytes, index, b" \t\r\n,;\"'");
        if token_end > index {
            output.extend_from_slice(REDACTED.as_bytes());
            index = token_end;
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| value.to_string())
}

fn redact_sensitive_assignments(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let Some(key_len) = match_sensitive_key(bytes, index) else {
            output.push(bytes[index]);
            index += 1;
            continue;
        };
        let mut cursor = index + key_len;
        let mut had_closing_key_quote = false;
        if cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'') {
            cursor += 1;
            had_closing_key_quote = true;
        }
        let after_key = skip_ascii_spaces(bytes, cursor);
        if after_key >= bytes.len() || !matches!(bytes[after_key], b':' | b'=') {
            output.extend_from_slice(&bytes[index..cursor]);
            index = cursor;
            continue;
        }

        output.extend_from_slice(&bytes[index..after_key + 1]);
        cursor = skip_ascii_spaces(bytes, after_key + 1);
        if cursor > after_key + 1 {
            output.extend_from_slice(&bytes[after_key + 1..cursor]);
        }

        let (quote, value_start) = consume_optional_quote(bytes, cursor);
        if let Some(quote) = quote {
            output.push(quote);
        }
        output.extend_from_slice(REDACTED.as_bytes());

        let value_end = if is_authorization_key(&bytes[index..index + key_len]) {
            scan_until_any(bytes, value_start, b"\r\n,;")
        } else {
            scan_until_value_delimiter(bytes, value_start, quote)
        };
        index = value_end;
        if let Some(quote) = quote
            && index < bytes.len()
            && bytes[index] == quote
        {
            output.push(bytes[index]);
            index += 1;
        }
        if had_closing_key_quote {
            // The key quote was copied with the key slice above; this flag exists to document
            // that JSON-style quoted keys are intentionally supported.
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| value.to_string())
}

fn match_sensitive_key(bytes: &[u8], index: usize) -> Option<usize> {
    SENSITIVE_KEYS.iter().find_map(|key| {
        let key_bytes = key.as_bytes();
        if !starts_with_ascii_case_insensitive(bytes, index, key_bytes) {
            return None;
        }
        let before_ok = index == 0 || is_key_boundary(bytes[index - 1]);
        let after_index = index + key_bytes.len();
        let after_ok = after_index >= bytes.len() || is_key_boundary(bytes[after_index]);
        (before_ok && after_ok).then_some(key_bytes.len())
    })
}

fn is_authorization_key(key: &[u8]) -> bool {
    key.eq_ignore_ascii_case(b"authorization")
}

fn skip_ascii_spaces(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    index
}

fn consume_optional_quote(bytes: &[u8], index: usize) -> (Option<u8>, usize) {
    if index < bytes.len() && matches!(bytes[index], b'"' | b'\'') {
        (Some(bytes[index]), index + 1)
    } else {
        (None, index)
    }
}

fn scan_until_value_delimiter(bytes: &[u8], index: usize, quote: Option<u8>) -> usize {
    if let Some(quote) = quote {
        scan_until_any(bytes, index, &[quote])
    } else {
        scan_until_any(bytes, index, b" \t\r\n,;&}]")
    }
}

fn scan_until_shell_delimiter(bytes: &[u8], index: usize, quote: Option<u8>) -> usize {
    if let Some(quote) = quote {
        scan_until_any(bytes, index, &[quote])
    } else {
        scan_until_any(bytes, index, b" \t\r\n")
    }
}

fn scan_until_any(bytes: &[u8], mut index: usize, delimiters: &[u8]) -> usize {
    while index < bytes.len()
        && !delimiters
            .iter()
            .any(|delimiter| *delimiter == bytes[index])
    {
        index += 1;
    }
    index
}

fn starts_with_ascii_case_insensitive(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes
        .get(index..index + needle.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle))
}

fn is_key_boundary(byte: u8) -> bool {
    !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_text_masks_common_secret_shapes() {
        let redacted = redact_sensitive_text(
            "curl -u 'user:pass123' https://user:pass123@example.com Authorization: Bearer abc token=xyz password=\"pw\"",
        );

        assert!(!redacted.contains("pass123"));
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("xyz"));
        assert!(!redacted.contains("\"pw\""));
        assert!(redacted.contains("<redacted>"));
    }
}
