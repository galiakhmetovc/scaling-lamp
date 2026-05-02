use agent_persistence::ArtifactRecord;
use serde_json::json;
use teloxide::types::Message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IncomingTelegramFile {
    pub(super) content_kind: &'static str,
    pub(super) file_id: String,
    pub(super) file_unique_id: String,
    pub(super) file_name: String,
    pub(super) mime_type: Option<String>,
    pub(super) size: u32,
}

pub(super) fn extract_incoming_file(message: &Message) -> Option<IncomingTelegramFile> {
    if let Some(document) = message.document() {
        return Some(IncomingTelegramFile {
            content_kind: "document",
            file_id: document.file.id.0.clone(),
            file_unique_id: document.file.unique_id.0.clone(),
            file_name: sanitize_file_name(
                document
                    .file_name
                    .as_deref()
                    .unwrap_or("telegram-document.bin"),
            ),
            mime_type: document.mime_type.as_ref().map(ToString::to_string),
            size: document.file.size,
        });
    }

    let photo = message
        .photo()
        .and_then(|photos| photos.iter().max_by_key(|photo| photo.file.size))?;
    Some(IncomingTelegramFile {
        content_kind: "photo",
        file_id: photo.file.id.0.clone(),
        file_unique_id: photo.file.unique_id.0.clone(),
        file_name: format!("telegram-photo-{}.jpg", message.id.0),
        mime_type: Some("image/jpeg".to_string()),
        size: photo.file.size,
    })
}

pub(super) fn render_uploaded_file_turn_input(
    artifact: &ArtifactRecord,
    file: &IncomingTelegramFile,
    caption: Option<&str>,
) -> String {
    let mut lines = vec![
        "Пользователь загрузил файл.".to_string(),
        format!("artifact_id={}", artifact.id),
        format!("kind={}", file.content_kind),
        format!("name={}", file.file_name),
        format!("size={}", artifact.bytes.len()),
    ];
    if let Some(mime_type) = file.mime_type.as_deref() {
        lines.push(format!("mime_type={mime_type}"));
    }
    if let Some(caption) = caption {
        lines.push(format!("caption={caption}"));
    }
    lines.push("Используй artifact_read, если нужно прочитать содержимое файла.".to_string());
    lines.join("\n")
}

pub(super) fn telegram_artifact_id(chat_id: i64, message_id: i32) -> String {
    let chat = chat_id.to_string().replace('-', "n");
    format!("artifact-telegram-{chat}-{message_id}")
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "telegram-file.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn artifact_metadata_value(artifact: &ArtifactRecord) -> serde_json::Value {
    serde_json::from_str(&artifact.metadata_json).unwrap_or_else(|_| json!({}))
}

pub(super) fn metadata_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}
