use super::app::BrowserItem;

#[derive(Debug)]
pub(super) struct ParsedAgentBrowser {
    pub(super) items: Vec<BrowserItem>,
    pub(super) selected_index: usize,
}

pub(super) fn parse_agent_browser_items(rendered: &str) -> ParsedAgentBrowser {
    let mut items = Vec::new();
    let mut selected_index = 0usize;
    for line in rendered.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("* ") {
            Some('*')
        } else if trimmed.starts_with("- ") {
            Some('-')
        } else {
            None
        };
        let Some(marker) = marker else {
            continue;
        };
        let Some((id, label)) = parse_agent_browser_line(trimmed) else {
            continue;
        };
        if marker == '*' {
            selected_index = items.len();
        }
        items.push(BrowserItem::new(id, label));
    }
    ParsedAgentBrowser {
        items,
        selected_index,
    }
}

fn parse_agent_browser_line(line: &str) -> Option<(String, String)> {
    let body = line
        .strip_prefix("* ")
        .or_else(|| line.strip_prefix("- "))?
        .trim();
    let id_start = body.rfind(" (")?;
    let id_end = body[id_start + 2..].find(')')? + id_start + 2;
    let id = body[id_start + 2..id_end].to_string();
    let label = body.to_string();
    Some((id, label))
}

pub(super) fn parse_schedule_browser_items(rendered: &str) -> Vec<BrowserItem> {
    parse_dash_prefixed_browser_items(rendered)
}

pub(super) fn parse_mcp_browser_items(rendered: &str) -> Vec<BrowserItem> {
    parse_dash_prefixed_browser_items(rendered)
}

pub(super) fn parse_artifact_browser_items(rendered: &str) -> Vec<BrowserItem> {
    parse_dash_prefixed_browser_items(rendered)
}

fn parse_dash_prefixed_browser_items(rendered: &str) -> Vec<BrowserItem> {
    rendered
        .lines()
        .filter_map(|line| {
            let body = line.trim_start().strip_prefix("- ")?;
            let id = body.split_whitespace().next()?.to_string();
            Some(BrowserItem::new(id, body.to_string()))
        })
        .collect()
}
