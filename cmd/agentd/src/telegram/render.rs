use crate::bootstrap::SessionSummary;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

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
    let html = render_markdown_to_telegram_html(text);
    if !html.is_empty() && html.len() <= soft_cap {
        return vec![TelegramRenderedChunk {
            text: html,
            parse_mode_html: true,
        }];
    }

    let plain = render_markdown_to_plain_text(text);
    chunk_message_text(&plain, soft_cap)
        .into_iter()
        .map(|text| TelegramRenderedChunk {
            text,
            parse_mode_html: false,
        })
        .collect()
}

pub fn render_markdown_to_telegram_html(markdown: &str) -> String {
    let mut output = String::new();
    let mut list_stack = Vec::<ListState>::new();
    let mut link_stack = Vec::<String>::new();

    for event in Parser::new_ext(markdown, Options::all()) {
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
    let mut output = String::new();
    let mut list_stack = Vec::<ListState>::new();

    for event in Parser::new_ext(markdown, Options::all()) {
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
