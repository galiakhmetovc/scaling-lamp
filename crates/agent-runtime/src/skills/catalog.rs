use crate::skills::parser::parse_skill_frontmatter;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillCatalog {
    pub entries: Vec<SkillSummary>,
    pub skipped: Vec<SkippedSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub skill_dir: PathBuf,
    pub skill_md_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedSkill {
    pub skill_dir: PathBuf,
    pub reason: String,
}

pub fn scan_skill_catalog(skills_dir: &Path) -> Result<SkillCatalog, std::io::Error> {
    if !skills_dir.exists() {
        return Ok(SkillCatalog::default());
    }

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut skill_dirs = fs::read_dir(skills_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    skill_dirs.sort();

    for skill_dir in skill_dirs {
        let skill_md_path = skill_dir.join("SKILL.md");
        if !skill_md_path.is_file() {
            continue;
        }

        let contents = match fs::read_to_string(&skill_md_path) {
            Ok(contents) => contents,
            Err(error) => {
                skipped.push(SkippedSkill {
                    skill_dir: skill_dir.clone(),
                    reason: format!("failed to read SKILL.md: {error}"),
                });
                continue;
            }
        };

        match parse_skill_frontmatter(&skill_md_path, &contents) {
            Ok(frontmatter) => entries.push(SkillSummary {
                name: frontmatter.name,
                description: frontmatter.description,
                skill_dir: skill_dir.clone(),
                skill_md_path,
            }),
            Err(reason) => skipped.push(SkippedSkill {
                skill_dir: skill_dir.clone(),
                reason,
            }),
        }
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(SkillCatalog { entries, skipped })
}
