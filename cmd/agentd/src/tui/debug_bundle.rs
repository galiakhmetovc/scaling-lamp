use super::app::TuiAppState;
use super::backend::TuiBackend;
use super::render;
use crate::bootstrap::BootstrapError;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn write_combined_tui_debug_bundle<B>(
    app: &B,
    state: &TuiAppState,
    session_id: &str,
    backend_debug_bundle_path: &str,
) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let saved_at = unix_timestamp()?;
    let output_path = tui_debug_bundle_output_path(session_id, backend_debug_bundle_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(BootstrapError::Stream)?;
    }

    let backend_context = app.render_context(session_id)?;
    let (terminal_width, terminal_height) =
        crossterm::terminal::size().map_or((0, 0), |(width, height)| (width, height));
    let viewport = render::chat_viewport_debug(
        state,
        ratatui::layout::Rect::new(0, 0, terminal_width, terminal_height),
    );
    let backend_bundle_contents = fs::read_to_string(backend_debug_bundle_path).ok();

    let mut lines = vec![
        "TUI Debug Bundle".to_string(),
        format!("generated_at={saved_at}"),
        format!("version={}", crate::about::APP_VERSION),
        format!("backend_debug_bundle_path={backend_debug_bundle_path}"),
        format!("screen={:?}", state.active_screen()),
        format!("session_id={session_id}"),
    ];

    if let Some(summary) = state.current_session_summary() {
        lines.push(format!("session_title={}", summary.title));
        match (
            summary.usage_input_tokens,
            summary.usage_output_tokens,
            summary.usage_total_tokens,
        ) {
            (Some(input), Some(output), Some(total)) => {
                lines.push(format!(
                    "summary_usage=input:{input} output:{output} total:{total}"
                ));
            }
            _ => lines.push(format!("summary_approx_ctx={}", summary.context_tokens)),
        }
        lines.push(format!("summary_messages={}", summary.message_count));
        lines.push(format!(
            "summary_reasoning_visible={}",
            summary.reasoning_visible
        ));
    }

    lines.push(String::new());
    lines.push("Viewport:".to_string());
    if let Some(viewport) = viewport {
        lines.push(format!("terminal_width={}", viewport.terminal_width));
        lines.push(format!("terminal_height={}", viewport.terminal_height));
        lines.push(format!("composer_height={}", viewport.composer_height));
        lines.push(format!(
            "timeline_viewport_width={}",
            viewport.timeline_viewport_width
        ));
        lines.push(format!(
            "timeline_viewport_height={}",
            viewport.timeline_viewport_height
        ));
        lines.push(format!(
            "timeline_total_lines={}",
            viewport.timeline_total_lines
        ));
        lines.push(format!(
            "timeline_scroll_top={}",
            viewport.timeline_scroll_top
        ));
        lines.push(format!("scroll_offset={}", viewport.scroll_offset));
        lines.push(format!("reasoning_visible={}", viewport.reasoning_visible));
        lines.push(format!(
            "visible_entry_count={}",
            viewport.visible_entry_count
        ));
        lines.push(format!("total_entry_count={}", viewport.total_entry_count));
    } else {
        lines.push("viewport_unavailable=true".to_string());
    }

    lines.push(String::new());
    lines.push("Composer:".to_string());
    lines.push(format!("input_cursor={}", state.input_cursor()));
    lines.push(format!("input_buffer_len={}", state.input_buffer().len()));
    lines.push(format!(
        "input_line_count={}",
        state.input_buffer().split('\n').count()
    ));
    lines.push(format!("queued_priority={}", state.queued_priority_count()));
    lines.push(format!("queued_deferred={}", state.queued_deferred_count()));

    lines.push(String::new());
    lines.push("Backend Context Snapshot:".to_string());
    lines.push(backend_context);

    lines.push(String::new());
    lines.push("Backend Bundle Contents:".to_string());
    match backend_bundle_contents {
        Some(contents) => lines.push(contents),
        None => lines.push("<unavailable from local TUI process>".to_string()),
    }

    fs::write(&output_path, lines.join("\n")).map_err(BootstrapError::Stream)?;
    Ok(output_path.display().to_string())
}

fn tui_debug_bundle_output_path(session_id: &str, backend_debug_bundle_path: &str) -> PathBuf {
    let backend_path = PathBuf::from(backend_debug_bundle_path);
    if backend_path.exists()
        && let Some(parent) = backend_path.parent()
    {
        let stem = backend_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("debug-bundle");
        return parent.join(format!("{stem}-tui.txt"));
    }

    let root = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    root.join(".teamd-debug").join(format!(
        "tui-{}-{}.txt",
        sanitize_tui_debug_identifier(session_id),
        unix_timestamp().unwrap_or_default()
    ))
}

fn sanitize_tui_debug_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
