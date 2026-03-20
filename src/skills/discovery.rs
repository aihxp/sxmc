use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Default skill search paths
pub fn default_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Personal skills
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".claude").join("skills"));
    }

    // Project skills (relative to cwd, canonicalized to absolute)
    let project_path = PathBuf::from(".claude").join("skills");
    if let Ok(abs) = project_path.canonicalize() {
        paths.push(abs);
    } else {
        paths.push(project_path);
    }

    paths
}

/// Discover skill directories across search paths.
/// Later paths take priority (project overrides personal).
pub fn discover_skills(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut skills: HashMap<String, PathBuf> = HashMap::new();

    for base_path in paths {
        if !base_path.exists() || !base_path.is_dir() {
            continue;
        }

        discover_in_dir(base_path, &mut skills)?;
    }

    let mut result: Vec<PathBuf> = skills.into_values().collect();
    result.sort();
    Ok(result)
}

fn discover_in_dir(dir: &Path, skills: &mut HashMap<String, PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    skills.insert(name.to_string(), path);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discover_skills_finds_skill_dirs() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\n---\nBody").unwrap();

        let result = discover_skills(&[tmp.path().to_path_buf()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_name().unwrap().to_str().unwrap(), "my-skill");
    }

    #[test]
    fn test_discover_skills_ignores_dirs_without_skill_md() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("not-a-skill");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "hi").unwrap();

        let result = discover_skills(&[tmp.path().to_path_buf()]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_project_overrides_personal() {
        let personal = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();

        // Same skill name in both
        for tmp in [&personal, &project] {
            let skill_dir = tmp.path().join("shared-skill");
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                "---\nname: shared-skill\n---\nBody",
            )
            .unwrap();
        }

        // Project comes after personal, so it wins
        let result =
            discover_skills(&[personal.path().to_path_buf(), project.path().to_path_buf()])
                .unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].starts_with(project.path()));
    }
}
