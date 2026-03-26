use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SxmcError};
use crate::paths::{InstallPaths, InstallScope};
use crate::skills::{discovery, parser};

const SKILL_INSTALL_METADATA_SCHEMA: &str = "sxmc_skill_install_v1";
const SKILL_INSTALL_METADATA_FILE: &str = ".sxmc-source.json";
type GithubTreeParse = (String, Option<String>, Option<String>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkillMetadata {
    pub schema: String,
    pub source: String,
    pub source_kind: String,
    pub repo_url: Option<String>,
    pub repo_subpath: Option<String>,
    pub reference: Option<String>,
    pub install_scope: String,
    pub installed_at: String,
}

#[derive(Debug, Clone)]
pub struct SkillInstallRequest<'a> {
    pub source: &'a str,
    pub repo_subpath: Option<&'a str>,
    pub reference: Option<&'a str>,
    pub install_paths: &'a InstallPaths,
    pub skills_path: &'a Path,
}

#[derive(Debug, Clone)]
pub struct SkillInstallReport {
    pub name: String,
    pub target_dir: PathBuf,
    pub install_scope: InstallScope,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct SkillUpdateRequest<'a> {
    pub name: Option<&'a str>,
    pub install_paths: &'a InstallPaths,
    pub skills_path: &'a Path,
}

#[derive(Debug, Clone)]
enum ResolvedSkillSource {
    LocalPath {
        original: String,
        path: PathBuf,
    },
    GitRepo {
        original: String,
        clone_url: String,
        repo_subpath: Option<String>,
        reference: Option<String>,
        source_kind: &'static str,
    },
}

impl InstalledSkillMetadata {
    pub fn metadata_path(skill_dir: &Path) -> PathBuf {
        skill_dir.join(SKILL_INSTALL_METADATA_FILE)
    }

    fn from_source(
        source: &ResolvedSkillSource,
        install_scope: InstallScope,
    ) -> InstalledSkillMetadata {
        match source {
            ResolvedSkillSource::LocalPath { original, .. } => InstalledSkillMetadata {
                schema: SKILL_INSTALL_METADATA_SCHEMA.to_string(),
                source: original.clone(),
                source_kind: "local_path".to_string(),
                repo_url: None,
                repo_subpath: None,
                reference: None,
                install_scope: install_scope.as_str().to_string(),
                installed_at: Utc::now().to_rfc3339(),
            },
            ResolvedSkillSource::GitRepo {
                original,
                clone_url,
                repo_subpath,
                reference,
                source_kind,
            } => InstalledSkillMetadata {
                schema: SKILL_INSTALL_METADATA_SCHEMA.to_string(),
                source: original.clone(),
                source_kind: (*source_kind).to_string(),
                repo_url: Some(clone_url.clone()),
                repo_subpath: repo_subpath.clone(),
                reference: reference.clone(),
                install_scope: install_scope.as_str().to_string(),
                installed_at: Utc::now().to_rfc3339(),
            },
        }
    }
}

pub fn install_skill(request: SkillInstallRequest<'_>) -> Result<SkillInstallReport> {
    let resolved = resolve_skill_source(request.source, request.repo_subpath, request.reference)?;
    let skill_source_dir = materialize_source_dir(&resolved)?;
    let parsed = parser::parse_skill(&skill_source_dir, request.source)?;
    let skills_root = request.install_paths.resolve_skills_path(request.skills_path);
    let target_dir = skills_root.join(&parsed.name);

    if target_dir.exists() {
        return Err(SxmcError::Other(format!(
            "Skill `{}` is already installed at {}. Re-run with `sxmc skills update {}` to refresh it.",
            parsed.name,
            target_dir.display(),
            parsed.name
        )));
    }

    fs::create_dir_all(&skills_root)?;
    install_skill_into_target(
        &skill_source_dir,
        &target_dir,
        &InstalledSkillMetadata::from_source(&resolved, request.install_paths.scope()),
    )?;

    Ok(SkillInstallReport {
        name: parsed.name,
        target_dir,
        install_scope: request.install_paths.scope(),
        source: request.source.to_string(),
    })
}

pub fn update_skills(request: SkillUpdateRequest<'_>) -> Result<Vec<SkillInstallReport>> {
    let skills_root = request.install_paths.resolve_skills_path(request.skills_path);
    if !skills_root.exists() {
        return Err(SxmcError::Other(format!(
            "No installed skills found at {}",
            skills_root.display()
        )));
    }

    let mut reports = Vec::new();
    let skill_dirs = discovery::discover_skills(std::slice::from_ref(&skills_root))?;

    for skill_dir in skill_dirs {
        let metadata = read_installed_skill_metadata(&skill_dir)?.ok_or_else(|| {
            SxmcError::Other(format!(
                "Skill at {} is not metadata-managed and cannot be updated automatically",
                skill_dir.display()
            ))
        })?;
        let parsed = parser::parse_skill(&skill_dir, skills_root.to_string_lossy().as_ref())?;
        if let Some(name) = request.name {
            if parsed.name != name {
                continue;
            }
        }

        let resolved = resolve_skill_source(
            &metadata.source,
            metadata.repo_subpath.as_deref(),
            metadata.reference.as_deref(),
        )?;
        let source_dir = materialize_source_dir(&resolved)?;
        let fresh = parser::parse_skill(&source_dir, &metadata.source)?;
        if fresh.name != parsed.name {
            return Err(SxmcError::Other(format!(
                "Updated skill source for `{}` now resolves to `{}`. Rename migrations are not automatic.",
                parsed.name, fresh.name
            )));
        }

        install_skill_into_target(
            &source_dir,
            &skill_dir,
            &InstalledSkillMetadata::from_source(&resolved, request.install_paths.scope()),
        )?;
        reports.push(SkillInstallReport {
            name: parsed.name,
            target_dir: skill_dir,
            install_scope: request.install_paths.scope(),
            source: metadata.source,
        });
    }

    if reports.is_empty() {
        if let Some(name) = request.name {
            return Err(SxmcError::SkillNotFound(name.to_string()));
        }
    }

    Ok(reports)
}

pub fn read_installed_skill_metadata(skill_dir: &Path) -> Result<Option<InstalledSkillMetadata>> {
    let metadata_path = InstalledSkillMetadata::metadata_path(skill_dir);
    if !metadata_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&metadata_path)?;
    let metadata: InstalledSkillMetadata = serde_json::from_str(&content)?;
    Ok(Some(metadata))
}

fn install_skill_into_target(
    source_dir: &Path,
    target_dir: &Path,
    metadata: &InstalledSkillMetadata,
) -> Result<()> {
    let parent = target_dir
        .parent()
        .ok_or_else(|| SxmcError::Other("Skill target has no parent directory".to_string()))?;
    fs::create_dir_all(parent)?;

    let staging = tempfile::tempdir_in(parent)?;
    let staged_dir = staging.path().join(
        target_dir
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("skill"),
    );
    copy_dir_recursive(source_dir, &staged_dir)?;

    let metadata_path = InstalledSkillMetadata::metadata_path(&staged_dir);
    fs::write(&metadata_path, serde_json::to_string_pretty(metadata)?)?;

    if target_dir.exists() {
        fs::remove_dir_all(target_dir)?;
    }
    fs::rename(&staged_dir, target_dir)?;
    Ok(())
}

fn copy_dir_recursive(source_dir: &Path, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)?;
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn materialize_source_dir(source: &ResolvedSkillSource) -> Result<PathBuf> {
    match source {
        ResolvedSkillSource::LocalPath { path, .. } => ensure_skill_dir(path),
        ResolvedSkillSource::GitRepo {
            clone_url,
            repo_subpath,
            reference,
            ..
        } => {
            let temp = tempfile::tempdir()?;
            let repo_dir = temp.path().join("repo");
            let mut command = Command::new("git");
            command.args(["clone", "--depth", "1"]);
            if let Some(reference) = reference {
                command.args(["--branch", reference]);
            }
            command.arg(clone_url).arg(&repo_dir);
            let output = command.output()?;
            if !output.status.success() {
                return Err(SxmcError::Other(format!(
                    "Failed to clone skill source `{}`: {}",
                    clone_url,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            let materialized = temp.keep();
            let resolved = match repo_subpath {
                Some(path) if !path.is_empty() => materialized.join(path),
                _ => materialized,
            };
            ensure_skill_dir(&resolved)
        }
    }
}

fn ensure_skill_dir(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !canonical.join("SKILL.md").exists() {
        return Err(SxmcError::Other(format!(
            "Skill source at {} does not contain SKILL.md",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn resolve_skill_source(
    source: &str,
    repo_subpath: Option<&str>,
    reference: Option<&str>,
) -> Result<ResolvedSkillSource> {
    let source_path = PathBuf::from(source);
    if source_path.exists() {
        return Ok(ResolvedSkillSource::LocalPath {
            original: source.to_string(),
            path: source_path,
        });
    }

    if let Some((clone_url, parsed_ref, parsed_subpath)) = parse_github_tree_url(source)? {
        return Ok(ResolvedSkillSource::GitRepo {
            original: source.to_string(),
            clone_url,
            repo_subpath: repo_subpath
                .map(ToOwned::to_owned)
                .or(parsed_subpath)
                .filter(|value| !value.is_empty()),
            reference: reference.map(ToOwned::to_owned).or(parsed_ref),
            source_kind: "github_tree",
        });
    }

    if looks_like_repo_locator(source) {
        let clone_url = if source.starts_with("http://")
            || source.starts_with("https://")
            || source.starts_with("git@")
        {
            source.to_string()
        } else {
            format!("https://github.com/{}.git", source)
        };
        return Ok(ResolvedSkillSource::GitRepo {
            original: source.to_string(),
            clone_url,
            repo_subpath: repo_subpath.map(ToOwned::to_owned),
            reference: reference.map(ToOwned::to_owned),
            source_kind: "git",
        });
    }

    Err(SxmcError::Other(format!(
        "Unsupported skill source `{}`. Use a local skill directory, a git URL, or a GitHub tree URL.",
        source
    )))
}

fn looks_like_repo_locator(source: &str) -> bool {
    if source.contains(' ') {
        return false;
    }
    source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
        || source.ends_with(".git")
        || source.split('/').count() == 2
}

fn parse_github_tree_url(source: &str) -> Result<Option<GithubTreeParse>> {
    let url = match url::Url::parse(source) {
        Ok(url) => url,
        Err(_) => return Ok(None),
    };
    if url.host_str() != Some("github.com") {
        return Ok(None);
    }
    let segments = match url.path_segments() {
        Some(segments) => segments.collect::<Vec<_>>(),
        None => return Ok(None),
    };
    if segments.len() < 4 || segments[2] != "tree" {
        return Ok(None);
    }
    let owner = segments[0];
    let repo = segments[1];
    let reference = segments[3].to_string();
    let subpath = if segments.len() > 4 {
        Some(segments[4..].join("/"))
    } else {
        None
    };
    Ok(Some((
        format!("https://github.com/{owner}/{repo}.git"),
        Some(reference),
        subpath,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_github_tree_url_extracts_ref_and_subpath() {
        let parsed = parse_github_tree_url(
            "https://github.com/openai/skills/tree/main/skills/.curated/example-skill",
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.0, "https://github.com/openai/skills.git");
        assert_eq!(parsed.1.as_deref(), Some("main"));
        assert_eq!(
            parsed.2.as_deref(),
            Some("skills/.curated/example-skill")
        );
    }

    #[test]
    fn test_read_installed_skill_metadata_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("demo-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let metadata = InstalledSkillMetadata {
            schema: SKILL_INSTALL_METADATA_SCHEMA.to_string(),
            source: "./source-skill".to_string(),
            source_kind: "local_path".to_string(),
            repo_url: None,
            repo_subpath: None,
            reference: None,
            install_scope: "local".to_string(),
            installed_at: Utc::now().to_rfc3339(),
        };
        fs::write(
            InstalledSkillMetadata::metadata_path(&skill_dir),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let loaded = read_installed_skill_metadata(&skill_dir).unwrap().unwrap();
        assert_eq!(loaded.source, "./source-skill");
        assert_eq!(loaded.install_scope, "local");
    }
}
