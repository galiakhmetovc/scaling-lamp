use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillFrontmatter {
    pub name: String,
    pub description: String,
}

pub fn parse_skill_frontmatter(
    path: &Path,
    contents: &str,
) -> Result<ParsedSkillFrontmatter, String> {
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
    for raw in lines {
        let line = raw.trim();
        if line == "---" {
            saw_closing = true;
            break;
        }
        if line.is_empty() || line.starts_with('#') {
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

    Ok(ParsedSkillFrontmatter { name, description })
}
