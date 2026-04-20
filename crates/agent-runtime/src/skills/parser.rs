use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillFrontmatter {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillDocument {
    pub frontmatter: ParsedSkillFrontmatter,
    pub body: String,
}

pub fn parse_skill_frontmatter(
    path: &Path,
    contents: &str,
) -> Result<ParsedSkillFrontmatter, String> {
    parse_skill_document(path, contents).map(|document| document.frontmatter)
}

pub fn parse_skill_document(path: &Path, contents: &str) -> Result<ParsedSkillDocument, String> {
    let mut lines = contents.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(format!(
            "skill {} is missing opening frontmatter fence",
            path.display()
        ));
    }

    let mut name = None;
    let mut description = None;
    let mut saw_closing = false;
    let mut body_start_offset = None;
    let mut scanned_len = contents
        .lines()
        .next()
        .map(|line| line.len() + 1)
        .unwrap_or_default();
    for raw in lines {
        let line = raw.trim();
        if line == "---" {
            saw_closing = true;
            body_start_offset = Some(scanned_len + raw.len() + 1);
            break;
        }
        if line.is_empty() || line.starts_with('#') {
            scanned_len += raw.len() + 1;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!(
                "skill {} has malformed frontmatter line: {line}",
                path.display()
            ));
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
        scanned_len += raw.len() + 1;
    }

    if !saw_closing {
        return Err(format!(
            "skill {} is missing closing frontmatter fence",
            path.display()
        ));
    }

    let name = name.ok_or_else(|| {
        format!(
            "skill {} frontmatter is missing required key name",
            path.display()
        )
    })?;
    let description = description.ok_or_else(|| {
        format!(
            "skill {} frontmatter is missing required key description",
            path.display()
        )
    })?;

    let body = body_start_offset
        .map(|offset| contents.get(offset..).unwrap_or_default())
        .unwrap_or_default()
        .trim()
        .to_string();

    Ok(ParsedSkillDocument {
        frontmatter: ParsedSkillFrontmatter { name, description },
        body,
    })
}
