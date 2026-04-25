use crate::bootstrap::SessionSummary;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

pub const TELEGRAM_MESSAGE_TEXT_SOFT_CAP: usize = 3_276;
pub const TELEGRAM_CAPTION_SOFT_CAP: usize = 819;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramRenderedChunk {
    pub text: String,
    pub parse_mode_html: bool,
}

pub fn chunk_message_text(text: &str, soft_cap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if current.len() >= soft_cap {
            chunks.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

pub fn truncate_caption(text: &str, soft_cap: usize) -> String {
    text.chars().take(soft_cap).collect()
}

pub fn render_model_response_chunks(text: &str, soft_cap: usize) -> Vec<TelegramRenderedChunk> {
    let markdown_chunks = split_markdown_render_chunks(text, soft_cap);
    let mut rendered = Vec::new();
    for markdown_chunk in markdown_chunks {
        let html = render_markdown_to_telegram_html(&markdown_chunk);
        if !html.is_empty() && html.len() <= soft_cap {
            rendered.push(TelegramRenderedChunk {
                text: html,
                parse_mode_html: true,
            });
            continue;
        }

        let plain = render_markdown_to_plain_text(&markdown_chunk);
        rendered.extend(
            chunk_message_text(&plain, soft_cap)
                .into_iter()
                .map(|text| TelegramRenderedChunk {
                    text,
                    parse_mode_html: false,
                }),
        );
    }

    if rendered.is_empty() {
        vec![TelegramRenderedChunk {
            text: String::new(),
            parse_mode_html: false,
        }]
    } else {
        rendered
    }
}

pub fn render_markdown_to_telegram_html(markdown: &str) -> String {
    let markdown = rewrite_markdown_tables_as_code_blocks(markdown);
    let mut output = String::new();
    let mut list_stack = Vec::<ListState>::new();
    let mut link_stack = Vec::<String>::new();

    for event in Parser::new_ext(&markdown, Options::all()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => start_block(&mut output),
                Tag::Heading { .. } => {
                    start_block(&mut output);
                    output.push_str("<b>");
                }
                Tag::BlockQuote(_) => {
                    start_block(&mut output);
                    output.push_str("<blockquote>");
                }
                Tag::CodeBlock(kind) => {
                    start_block(&mut output);
                    match kind {
                        CodeBlockKind::Indented => output.push_str("<pre><code>"),
                        CodeBlockKind::Fenced(language) => {
                            if let Some(language) = language.split_whitespace().next() {
                                if !language.is_empty() {
                                    output.push_str("<pre><code class=\"language-");
                                    output.push_str(&escape_html_attribute(language));
                                    output.push_str("\">");
                                } else {
                                    output.push_str("<pre><code>");
                                }
                            } else {
                                output.push_str("<pre><code>");
                            }
                        }
                    }
                }
                Tag::List(first_ordinal) => list_stack.push(ListState {
                    next_ordinal: first_ordinal,
                }),
                Tag::Item => start_list_item(&mut output, &mut list_stack),
                Tag::Emphasis => output.push_str("<i>"),
                Tag::Strong => output.push_str("<b>"),
                Tag::Strikethrough => output.push_str("<s>"),
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                    let destination = escape_html_attribute(dest_url.as_ref());
                    output.push_str("<a href=\"");
                    output.push_str(&destination);
                    output.push_str("\">");
                    link_stack.push(destination);
                }
                Tag::FootnoteDefinition(_) => start_block(&mut output),
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => end_block(&mut output),
                TagEnd::Heading(_) => {
                    output.push_str("</b>");
                    end_block(&mut output);
                }
                TagEnd::BlockQuote(_) => {
                    output.push_str("</blockquote>");
                    end_block(&mut output);
                }
                TagEnd::CodeBlock => {
                    output.push_str("</code></pre>");
                    end_block(&mut output);
                }
                TagEnd::List(_) => {
                    trim_trailing_newline(&mut output);
                    end_block(&mut output);
                    list_stack.pop();
                }
                TagEnd::Item => trim_trailing_spaces(&mut output),
                TagEnd::Emphasis => output.push_str("</i>"),
                TagEnd::Strong => output.push_str("</b>"),
                TagEnd::Strikethrough => output.push_str("</s>"),
                TagEnd::Link | TagEnd::Image => {
                    output.push_str("</a>");
                    link_stack.pop();
                }
                _ => {}
            },
            Event::Text(text) => output.push_str(&escape_html_text(text.as_ref())),
            Event::Code(text) => {
                output.push_str("<code>");
                output.push_str(&escape_html_text(text.as_ref()));
                output.push_str("</code>");
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                output.push_str("<code>");
                output.push_str(&escape_html_text(text.as_ref()));
                output.push_str("</code>");
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                output.push_str(&escape_html_text(text.as_ref()));
            }
            Event::SoftBreak | Event::HardBreak => output.push('\n'),
            Event::Rule => {
                start_block(&mut output);
                output.push_str("────────");
                end_block(&mut output);
            }
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "☑ " } else { "☐ " });
            }
            Event::FootnoteReference(name) => {
                output.push('[');
                output.push_str(&escape_html_text(name.as_ref()));
                output.push(']');
            }
        }
    }

    output.trim().to_string()
}

pub fn render_markdown_to_plain_text(markdown: &str) -> String {
    let markdown = rewrite_markdown_tables_as_code_blocks(markdown);
    let mut output = String::new();
    let mut list_stack = Vec::<ListState>::new();

    for event in Parser::new_ext(&markdown, Options::all()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => start_block(&mut output),
                Tag::Heading { .. } => start_block(&mut output),
                Tag::BlockQuote(_) => {
                    start_block(&mut output);
                    output.push_str("> ");
                }
                Tag::CodeBlock(_) => start_block(&mut output),
                Tag::List(first_ordinal) => list_stack.push(ListState {
                    next_ordinal: first_ordinal,
                }),
                Tag::Item => start_list_item(&mut output, &mut list_stack),
                Tag::FootnoteDefinition(_) => start_block(&mut output),
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_) => {
                    end_block(&mut output);
                }
                TagEnd::List(_) => {
                    trim_trailing_newline(&mut output);
                    end_block(&mut output);
                    list_stack.pop();
                }
                TagEnd::Item => trim_trailing_spaces(&mut output),
                _ => {}
            },
            Event::Text(text)
            | Event::Code(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::Html(text)
            | Event::InlineHtml(text) => output.push_str(text.as_ref()),
            Event::SoftBreak | Event::HardBreak => output.push('\n'),
            Event::Rule => {
                start_block(&mut output);
                output.push_str("--------");
                end_block(&mut output);
            }
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::FootnoteReference(name) => {
                output.push('[');
                output.push_str(name.as_ref());
                output.push(']');
            }
        }
    }

    output.trim().to_string()
}

pub fn render_pairing_message(token: &str) -> String {
    format!("Pairing key: {token}\n\nActivate it on the server:\nagentd telegram pair {token}")
}

pub fn render_pairing_required_message() -> String {
    "Pairing required. Send /start to get a key.".to_string()
}

pub fn render_help_message() -> String {
    [
        "Telegram commands:",
        "/start - get a pairing key",
        "/help - show this help",
        "/new [title] - create and select a session",
        "/sessions - list sessions",
        "/use <session_id> - select a session",
        "/judge <message> - send a message to the Judge agent",
        "/agent <agent_id> <message> - send a message to another agent",
        "",
        "After pairing, plain text goes to the selected session.",
    ]
    .join("\n")
}

pub fn render_session_created(summary: &SessionSummary) -> String {
    format!("Selected session: {} ({})", summary.title, summary.id)
}

pub fn render_session_selected(summary: &SessionSummary) -> String {
    format!("Using session: {} ({})", summary.title, summary.id)
}

pub fn render_session_list(
    summaries: &[SessionSummary],
    selected_session_id: Option<&str>,
) -> String {
    if summaries.is_empty() {
        return "No sessions yet. Use /new to create one.".to_string();
    }

    let mut lines = vec!["Sessions:".to_string()];
    for summary in summaries {
        let marker = if selected_session_id == Some(summary.id.as_str()) {
            "*"
        } else {
            "-"
        };
        lines.push(format!(
            "{marker} {} ({}) messages={} autoapprove={}",
            summary.title, summary.id, summary.message_count, summary.auto_approve
        ));
    }

    lines.join("\n")
}

pub fn render_usage(command: &str, usage: &str) -> String {
    format!("Usage: /{command} {usage}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListState {
    next_ordinal: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MarkdownRenderBlock {
    Text(String),
    Table(Vec<Vec<String>>),
}

fn split_markdown_render_chunks(markdown: &str, soft_cap: usize) -> Vec<String> {
    if markdown.trim().is_empty() {
        return vec![String::new()];
    }

    let blocks = parse_markdown_render_blocks(markdown);
    let mut chunks = Vec::new();
    let mut current = String::new();

    for block in blocks {
        for block_markdown in normalize_markdown_block_for_soft_cap(block, soft_cap) {
            if current.is_empty() {
                current = block_markdown;
                continue;
            }

            let candidate = format!("{current}\n\n{block_markdown}");
            if render_markdown_to_telegram_html(&candidate).len() <= soft_cap {
                current = candidate;
            } else {
                chunks.push(std::mem::take(&mut current));
                current = block_markdown;
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }
}

fn parse_markdown_render_blocks(markdown: &str) -> Vec<MarkdownRenderBlock> {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut current_text = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        if line.trim_start().starts_with("```") {
            flush_text_markdown_block(&mut blocks, &mut current_text);
            let mut code_lines = vec![line.to_string()];
            index += 1;
            while index < lines.len() {
                let code_line = lines[index];
                code_lines.push(code_line.to_string());
                index += 1;
                if code_line.trim_start().starts_with("```") {
                    break;
                }
            }
            blocks.push(MarkdownRenderBlock::Text(code_lines.join("\n")));
            continue;
        }

        if let Some((rows, next_index)) = collect_markdown_table_block(&lines, index) {
            flush_text_markdown_block(&mut blocks, &mut current_text);
            blocks.push(MarkdownRenderBlock::Table(rows));
            index = next_index;
            continue;
        }

        if line.trim().is_empty() {
            flush_text_markdown_block(&mut blocks, &mut current_text);
            index += 1;
            continue;
        }

        current_text.push(line.to_string());
        index += 1;
    }

    flush_text_markdown_block(&mut blocks, &mut current_text);
    blocks
}

fn flush_text_markdown_block(
    blocks: &mut Vec<MarkdownRenderBlock>,
    current_text: &mut Vec<String>,
) {
    if current_text.is_empty() {
        return;
    }
    blocks.push(MarkdownRenderBlock::Text(current_text.join("\n")));
    current_text.clear();
}

fn normalize_markdown_block_for_soft_cap(
    block: MarkdownRenderBlock,
    soft_cap: usize,
) -> Vec<String> {
    match block {
        MarkdownRenderBlock::Text(text) => vec![text],
        MarkdownRenderBlock::Table(rows) => split_markdown_table_for_soft_cap(&rows, soft_cap),
    }
}

fn split_markdown_table_for_soft_cap(rows: &[Vec<String>], soft_cap: usize) -> Vec<String> {
    if rows.len() <= 1 {
        return vec![markdown_table_to_markdown(rows)];
    }

    let header = rows[0].clone();
    let mut chunks = Vec::new();
    let mut current_rows = vec![header.clone()];

    for row in rows.iter().skip(1) {
        let mut candidate_rows = current_rows.clone();
        candidate_rows.push(row.clone());
        let candidate = markdown_table_to_markdown(&candidate_rows);

        if render_markdown_to_telegram_html(&candidate).len() <= soft_cap {
            current_rows = candidate_rows;
            continue;
        }

        if current_rows.len() > 1 {
            chunks.push(markdown_table_to_markdown(&current_rows));
            current_rows = vec![header.clone(), row.clone()];
            continue;
        }

        chunks.push(candidate);
        current_rows = vec![header.clone()];
    }

    if current_rows.len() > 1 {
        chunks.push(markdown_table_to_markdown(&current_rows));
    }

    if chunks.is_empty() {
        vec![markdown_table_to_markdown(rows)]
    } else {
        chunks
    }
}

fn markdown_table_to_markdown(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(markdown_table_row(&rows[0]));
    lines.push(markdown_table_separator(rows[0].len()));
    for row in rows.iter().skip(1) {
        lines.push(markdown_table_row(row));
    }
    lines.join("\n")
}

fn markdown_table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

fn markdown_table_separator(column_count: usize) -> String {
    format!("| {} |", vec!["---"; column_count].join(" | "))
}

fn rewrite_markdown_tables_as_code_blocks(markdown: &str) -> String {
    if !markdown.contains('|') || !markdown.contains('-') {
        return markdown.to_string();
    }

    let lines = markdown.lines().collect::<Vec<_>>();
    let had_trailing_newline = markdown.ends_with('\n');
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0;
    let mut in_code_fence = false;

    while index < lines.len() {
        let line = lines[index];
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        if !in_code_fence
            && let Some((table_rows, next_index)) = collect_markdown_table_block(&lines, index)
        {
            rewritten.push("```".to_string());
            rewritten.extend(render_markdown_table_block(&table_rows));
            rewritten.push("```".to_string());
            index = next_index;
            continue;
        }

        rewritten.push(line.to_string());
        index += 1;
    }

    let mut result = rewritten.join("\n");
    if had_trailing_newline {
        result.push('\n');
    }
    result
}

fn collect_markdown_table_block(lines: &[&str], start: usize) -> Option<(Vec<Vec<String>>, usize)> {
    let header = lines.get(start)?.trim();
    let separator = lines.get(start + 1)?.trim();
    if !looks_like_markdown_table_row(header) || !looks_like_markdown_table_separator(separator) {
        return None;
    }

    let mut rows = vec![parse_markdown_table_cells(header)];
    let mut index = start + 2;
    while index < lines.len() {
        let row = lines[index].trim();
        if !looks_like_markdown_table_row(row) {
            break;
        }
        rows.push(parse_markdown_table_cells(row));
        index += 1;
    }

    Some((rows, index))
}

fn looks_like_markdown_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.contains('|')
}

fn looks_like_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !looks_like_markdown_table_row(trimmed) {
        return false;
    }

    let cells = trimmed
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect::<Vec<_>>();
    if cells.len() < 2 {
        return false;
    }

    cells
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn parse_markdown_table_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| inline_markdown_plain_text(cell.trim()))
        .collect()
}

fn render_markdown_table_block(rows: &[Vec<String>]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }

    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let normalized = rows
        .iter()
        .map(|row| {
            (0..column_count)
                .map(|column| row.get(column).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut widths = vec![0; column_count];
    for row in &normalized {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let mut lines = Vec::with_capacity(normalized.len() + 1);
    for (index, row) in normalized.iter().enumerate() {
        lines.push(format_markdown_table_row(row, &widths));
        if index == 0 {
            lines.push(format_markdown_table_separator(&widths));
        }
    }
    lines
}

fn format_markdown_table_row(row: &[String], widths: &[usize]) -> String {
    let cells = row
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let width = widths.get(index).copied().unwrap_or_default();
            format!(" {}{} ", cell, pad_display_width(cell, width))
        })
        .collect::<Vec<_>>();
    format!("|{}|", cells.join("|"))
}

fn format_markdown_table_separator(widths: &[usize]) -> String {
    let cells = widths
        .iter()
        .map(|width| format!(" {} ", "-".repeat((*width).max(1))))
        .collect::<Vec<_>>();
    format!("|{}|", cells.join("|"))
}

fn inline_markdown_plain_text(content: &str) -> String {
    let mut output = String::new();
    for event in Parser::new_ext(content, Options::all()) {
        match event {
            Event::Text(text)
            | Event::Code(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::Html(text)
            | Event::InlineHtml(text) => output.push_str(text.as_ref()),
            Event::SoftBreak | Event::HardBreak => output.push(' '),
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::FootnoteReference(name) => {
                output.push('[');
                output.push_str(name.as_ref());
                output.push(']');
            }
            _ => {}
        }
    }
    output
}

fn pad_display_width(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    " ".repeat(width.saturating_sub(current))
}

fn start_block(output: &mut String) {
    if output.is_empty() {
        return;
    }

    if output.ends_with("\n\n") {
        return;
    }

    if output.ends_with('\n') {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

fn end_block(output: &mut String) {
    trim_trailing_spaces(output);
    if output.is_empty() || output.ends_with("\n\n") {
        return;
    }
    if output.ends_with('\n') {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

fn start_list_item(output: &mut String, list_stack: &mut [ListState]) {
    trim_trailing_spaces(output);
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }

    if list_stack.len() > 1 {
        output.push_str(&"  ".repeat(list_stack.len() - 1));
    }

    if let Some(last) = list_stack.last_mut() {
        if let Some(next_ordinal) = last.next_ordinal.as_mut() {
            output.push_str(&format!("{next_ordinal}. "));
            *next_ordinal += 1;
        } else {
            output.push_str("• ");
        }
    } else {
        output.push_str("• ");
    }
}

fn trim_trailing_spaces(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
}

fn trim_trailing_newline(output: &mut String) {
    while output.ends_with('\n') {
        output.pop();
    }
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attribute(text: &str) -> String {
    escape_html_text(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{
        TELEGRAM_MESSAGE_TEXT_SOFT_CAP, render_markdown_to_telegram_html,
        render_model_response_chunks,
    };

    #[test]
    fn render_model_response_chunks_splits_large_tables_into_html_chunks() {
        let mut markdown =
            String::from("## Прогноз\n\n| Параметр | Утро | День | Вечер |\n|---|---|---|---|\n");
        for index in 0..20 {
            markdown.push_str(&format!(
                "| Строка {index} | +{index}° | дождь | ветер {index} |\n"
            ));
        }

        let chunks = render_model_response_chunks(&markdown, 260);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.parse_mode_html));
        assert!(chunks.iter().all(|chunk| chunk.text.len() <= 260));
        assert!(chunks.iter().all(|chunk| chunk.text.contains("Параметр")));

        let table_chunks = chunks
            .iter()
            .filter(|chunk| chunk.text.contains("<pre><code>"))
            .collect::<Vec<_>>();
        assert!(table_chunks.len() >= 2);
        assert!(
            table_chunks
                .iter()
                .all(|chunk| chunk.text.contains("| Параметр"))
        );
    }

    #[test]
    fn render_model_response_chunks_keeps_short_html_responses_in_html_mode() {
        let markdown = "| A | B |\n|---|---|\n| 1 | 2 |";
        let chunks = render_model_response_chunks(markdown, TELEGRAM_MESSAGE_TEXT_SOFT_CAP);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].parse_mode_html);
        assert_eq!(chunks[0].text, render_markdown_to_telegram_html(markdown));
    }

    #[test]
    fn render_model_response_chunks_keeps_long_multi_table_responses_in_html_mode() {
        let markdown = r#"Теперь у меня есть данные из 3 источников. Формирую полный прогноз.

## Прогноз погоды

### Сводка

| Параметр | Утром | Днём | Вечером | Ночью |
|---|---|---|---|---|
| Температура | от +6 до +9° | от +10 до +11° | от +5 до +6° | от +2 до +7° |
| Ощущается как | от +2 до +4° | от +6 до +7° | от -2 до +1° | от -3 до +4° |
| Осадки | Дождь | Ливневый дождь | Дождь → снег | Снег |
| Ветер | 4–5 м/с | 5–6 м/с | 5–6 м/с | 4–6 м/с |
| Давление | 744–748 | 743–744 | 743–744 | 743–746 |
| Влажность | 83–86% | 68–77% | 77–91% | 74–87% |

### Расхождения между источниками

| Источник | Мин/Макс | Днём | Ветер | Особенности |
|---|---|---|---|---|
| wttr.in | +1 / +10 | Слабый ливневой дождь | 6,4 м/с | Снег к вечеру |
| Яндекс.Погода | +1 / +11 | Небольшой дождь | 6 м/с | Слабая магнитная буря |
| Gismeteo | +1 / +11 | Небольшой дождь | — | Выпадающий снег вечером |

> Ключевой вывод: день дождливый, вечером переход в снег.

### Дополнительно
- Восход: 04:58
- Закат: 19:56
- Магнитное поле: слабая буря
"#;

        let chunks = render_model_response_chunks(markdown, 600);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.parse_mode_html));
        assert!(chunks.iter().all(|chunk| chunk.text.len() <= 600));
        assert!(chunks.iter().any(|chunk| chunk.text.contains("Параметр")));
        assert!(chunks.iter().any(|chunk| chunk.text.contains("Источник")));
    }
}
