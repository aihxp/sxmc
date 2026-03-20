use std::path::Path;

use crate::error::{Result, SxmcError};
use crate::skills::models::*;

/// Split SKILL.md content into YAML frontmatter and markdown body.
pub fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Ok((String::new(), content.to_string()));
    }

    // Find closing ---
    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);

    if let Some(end_pos) = rest.find("\n---") {
        let yaml = rest[..end_pos].to_string();
        let body = rest[end_pos + 4..]
            .trim_start_matches(['\r', '\n'])
            .to_string();
        Ok((yaml, body))
    } else {
        Err(SxmcError::ParseError(
            "No closing --- found for frontmatter".to_string(),
        ))
    }
}

/// Parse a SKILL.md file into a Skill struct.
pub fn parse_skill(skill_dir: &Path, source: &str) -> Result<Skill> {
    let skill_dir = skill_dir.canonicalize().unwrap_or_else(|_| skill_dir.to_path_buf());
    let skill_md = skill_dir.join("SKILL.md");
    let content = std::fs::read_to_string(&skill_md)?;

    let (yaml_str, body) = split_frontmatter(&content)?;

    let frontmatter: SkillFrontmatter = if yaml_str.is_empty() {
        return Err(SxmcError::ParseError(
            "SKILL.md must have YAML frontmatter".to_string(),
        ));
    } else {
        serde_yaml::from_str(&yaml_str)?
    };

    let body = body.replace("${CLAUDE_SKILL_DIR}", skill_dir.to_str().unwrap_or(""));

    let scripts = scan_scripts(&skill_dir)?;
    let references = scan_references(&skill_dir, &frontmatter.name)?;

    Ok(Skill {
        name: frontmatter.name.clone(),
        base_dir: skill_dir,
        frontmatter,
        body,
        scripts,
        references,
        source: source.to_string(),
    })
}

fn scan_scripts(skill_dir: &Path) -> Result<Vec<SkillScript>> {
    let scripts_dir = skill_dir.join("scripts");
    if !scripts_dir.exists() {
        return Ok(Vec::new());
    }

    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(&scripts_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                scripts.push(SkillScript {
                    name: name.to_string(),
                    path,
                });
            }
        }
    }
    scripts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scripts)
}

fn scan_references(skill_dir: &Path, skill_name: &str) -> Result<Vec<SkillReference>> {
    let refs_dir = skill_dir.join("references");
    if !refs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut refs = Vec::new();
    for entry in std::fs::read_dir(&refs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name = name.to_string();
                let uri = format!("skill://{}/references/{}", skill_name, name);
                refs.push(SkillReference { name, path, uri });
            }
        }
    }
    refs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(refs)
}

/// Parse argument-hint into structured arguments.
/// Supports both `<required>` and `[optional]` forms, while preserving
/// the older convention where bracketed non-flag values are treated as required.
pub fn parse_argument_hint(hint: &str) -> Vec<SkillArgument> {
    let mut args = Vec::new();

    for cap in regex::Regex::new(r"<([^>]+)>|\[([^\]]+)\]")
        .unwrap()
        .captures_iter(hint)
    {
        let (token, required) = if let Some(required) = cap.get(1) {
            (required.as_str().trim(), true)
        } else {
            let optional = cap.get(2).unwrap().as_str().trim();
            (optional, !optional.starts_with('-'))
        };
        let is_flag = token.starts_with('-');
        let name = token.trim_start_matches('-').replace([' ', '-'], "_");

        args.push(SkillArgument {
            name,
            required: required && !is_flag,
            description: token.to_string(),
        });
    }

    if args.is_empty() {
        args.push(SkillArgument {
            name: "arguments".to_string(),
            required: false,
            description: "Arguments to pass".to_string(),
        });
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_split_frontmatter() {
        let content = "---\nname: test\ndescription: A test\n---\nThis is the body.";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert!(yaml.contains("name: test"));
        assert_eq!(body, "This is the body.");
    }

    #[test]
    fn test_split_frontmatter_no_frontmatter() {
        let content = "Just a body with no frontmatter.";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert!(yaml.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::create_dir_all(skill_dir.join("references")).unwrap();

        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\nargument-hint: \"[pr-number]\"\n---\nReview PR #$ARGUMENTS",
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/check.sh"), "#!/bin/bash\necho ok").unwrap();
        fs::write(skill_dir.join("references/guide.md"), "# Guide").unwrap();

        let skill = parse_skill(&skill_dir, "test").unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.frontmatter.description, "A test skill");
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(skill.references.len(), 1);
        assert_eq!(
            skill.references[0].uri,
            "skill://my-skill/references/guide.md"
        );
    }

    #[test]
    fn test_parse_argument_hint() {
        let args = parse_argument_hint("[pr-number] [--verbose]");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "pr_number");
        assert!(args[0].required);
        assert_eq!(args[1].name, "verbose");
        assert!(!args[1].required);
    }

    #[test]
    fn test_parse_argument_hint_empty() {
        let args = parse_argument_hint("");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "arguments");
        assert!(!args[0].required);
    }

    #[test]
    fn test_parse_argument_hint_angle_brackets() {
        let args = parse_argument_hint("<repo> [--dry-run]");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "repo");
        assert!(args[0].required);
        assert_eq!(args[1].name, "dry_run");
        assert!(!args[1].required);
    }
}
