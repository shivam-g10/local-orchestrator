use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPolicy {
    pub base_dir: PathBuf,
    pub max_depth: usize,
    pub max_files: usize,
    pub max_bytes_per_file_sample: usize,
    pub max_file_bytes_for_grep: u64,
    pub deny_globs: Vec<String>,
}

impl FsPolicy {
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self> {
        let base_dir =
            dunce::canonicalize(base_dir.into()).context("failed to canonicalize base_dir")?;

        Ok(Self {
            base_dir,
            max_depth: 12,
            max_files: 20_000,
            max_bytes_per_file_sample: 64 * 1024,
            max_file_bytes_for_grep: 2 * 1024 * 1024,
            deny_globs: default_denies(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String, // relative to root
    pub kind: NodeKind,
    pub size_bytes: Option<u64>,
    pub modified_utc: Option<DateTime<Utc>>,
    pub ext: Option<String>,
    pub hash8: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSample {
    pub path: String, // relative to root
    pub truncated: bool,
    pub head_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderStats {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_bytes: u64,
    pub by_extension: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDigest {
    pub requested_root: String,
    pub canonical_root: String,
    pub stats: FolderStats,
    pub nodes: Vec<FileNode>,
    pub samples: Vec<FileSample>,
    pub signals: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: String, // relative to root
    pub line: u64,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    pub canonical_root: String,
    pub query: String,
    pub matches: Vec<GrepMatch>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFacts {
    pub canonical_root: String,
    pub rust: Option<RustFacts>,
    pub docker: DockerFacts,
    pub env_vars: BTreeSet<String>,
    pub urls: BTreeSet<String>,
    pub ports: BTreeSet<u16>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustFacts {
    pub has_cargo_toml: bool,
    pub crates: BTreeSet<String>,
    pub binaries: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerFacts {
    pub has_dockerfile: bool,
    pub compose_files: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct FsTools {
    policy: FsPolicy,
    deny_set: GlobSet,
}

impl FsTools {
    pub fn new(policy: FsPolicy) -> Result<Self> {
        let deny_set = build_globset(&policy.deny_globs)?;
        Ok(Self { policy, deny_set })
    }

    /// Resolve a user path (absolute or relative) into a canonical path under base_dir.
    pub fn resolve_under_base(&self, input: impl AsRef<str>) -> Result<PathBuf> {
        let input = input.as_ref();
        let p = PathBuf::from(input);
        let joined = if p.is_absolute() {
            p
        } else {
            self.policy.base_dir.join(p)
        };

        let canon = dunce::canonicalize(&joined)
            .with_context(|| format!("path not found or cannot be resolved: {input}"))?;

        if !canon.starts_with(&self.policy.base_dir) {
            return Err(anyhow!(
                "path outside base_dir allowlist: input={input}, resolved={}",
                canon.display()
            ));
        }

        // Hard deny: .ssh etc.
        let s = canon.to_string_lossy();
        if s.contains("/.ssh/") || s.contains("\\.ssh\\") {
            return Err(anyhow!("access denied: .ssh"));
        }

        Ok(canon)
    }

    /// Folder digest: tree + stats + curated samples (smart selection).
    pub fn folder_digest(&self, root: impl AsRef<str>) -> Result<FolderDigest> {
        let requested_root = root.as_ref().to_string();
        let root = self.resolve_under_base(&requested_root)?;
        let canonical_root = root.display().to_string();

        let mut warnings = Vec::new();
        let mut nodes = Vec::new();
        let mut file_count: u64 = 0;
        let mut dir_count: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut by_extension: BTreeMap<String, u64> = BTreeMap::new();
        let mut files_set: BTreeSet<String> = BTreeSet::new();

        let mut wb = WalkBuilder::new(&root);
        wb.hidden(false)
            .follow_links(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .max_depth(Some(self.policy.max_depth));

        for entry in wb.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();

            if path == root.as_path() {
                continue;
            }

            let rel = match path.strip_prefix(&root) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // deny globs (relative)
            if self.deny_set.is_match(rel) {
                continue;
            }

            let rel_str = rel.to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);

            if is_dir {
                dir_count += 1;
                nodes.push(FileNode {
                    path: rel_str,
                    kind: NodeKind::Dir,
                    size_bytes: None,
                    modified_utc: None,
                    ext: None,
                    hash8: None,
                });
                continue;
            }
            if !is_file {
                continue;
            }

            file_count += 1;
            if file_count as usize > self.policy.max_files {
                warnings.push(format!(
                    "file limit reached (max_files={}); output truncated",
                    self.policy.max_files
                ));
                break;
            }

            let md = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = md.len();
            total_bytes = total_bytes.saturating_add(size);

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());
            *by_extension
                .entry(ext.clone().unwrap_or_default())
                .or_insert(0) += 1;

            let modified_utc = md.modified().ok().map(DateTime::<Utc>::from);
            let hash8 = hash_prefix(path, 4096).ok();

            nodes.push(FileNode {
                path: rel_str.clone(),
                kind: NodeKind::File,
                size_bytes: Some(size),
                modified_utc,
                ext,
                hash8,
            });

            files_set.insert(rel_str);
        }

        let signals = detect_signals(&files_set);
        let samples = sample_smart(&root, &files_set, self.policy.max_bytes_per_file_sample);

        Ok(FolderDigest {
            requested_root,
            canonical_root,
            stats: FolderStats {
                file_count,
                dir_count,
                total_bytes,
                by_extension,
            },
            nodes,
            samples,
            signals,
            warnings,
        })
    }

    /// Read a UTF-8 text chunk with offset/limit.
    pub fn read_file_chunk(
        &self,
        path: impl AsRef<str>,
        offset: u64,
        max_bytes: u64,
    ) -> Result<String> {
        let path_in = path.as_ref();
        let canon = self.resolve_under_base(path_in)?;

        let max_bytes = max_bytes.min(2 * 1024 * 1024);
        let mut f = File::open(&canon).with_context(|| format!("open failed: {path_in}"))?;

        // discard to offset
        let mut discarded = 0u64;
        let mut tmp = [0u8; 8192];
        while discarded < offset {
            let to_read = ((offset - discarded) as usize).min(tmp.len());
            let n = f.read(&mut tmp[..to_read])?;
            if n == 0 {
                break;
            }
            discarded += n as u64;
        }

        let mut buf = vec![0u8; max_bytes as usize];
        let n = f.read(&mut buf)?;
        buf.truncate(n);

        if buf.contains(&0) {
            return Err(anyhow!(
                "binary file (NUL byte detected): {}",
                canon.display()
            ));
        }
        let s = std::str::from_utf8(&buf).context("not UTF-8")?;
        Ok(s.to_string())
    }

    /// Grep for substring across files under root. Skips binary and large files.
    pub fn grep(
        &self,
        root: impl AsRef<str>,
        query: &str,
        case_sensitive: bool,
        max_matches: usize,
    ) -> Result<GrepResult> {
        let requested_root = root.as_ref().to_string();
        let root = self.resolve_under_base(&requested_root)?;
        let canonical_root = root.display().to_string();

        let max_matches = max_matches.min(5000);
        let needle = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        let mut out = Vec::new();
        let mut warnings = Vec::new();

        let mut wb = WalkBuilder::new(&root);
        wb.hidden(false)
            .follow_links(false)
            .max_depth(Some(self.policy.max_depth));

        'walk: for entry in wb.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }

            let path = entry.path();
            let rel = match path.strip_prefix(&root) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if self.deny_set.is_match(rel) {
                continue;
            }

            let md = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.len() > self.policy.max_file_bytes_for_grep {
                continue;
            }

            // binary check
            if is_binary_prefix(path, 2048) {
                continue;
            }

            let f = match File::open(path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let reader = BufReader::new(f);

            for (idx, line_res) in reader.lines().enumerate() {
                let line = match line_res {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                let hay = if case_sensitive {
                    line.clone()
                } else {
                    line.to_lowercase()
                };
                if hay.contains(&needle) {
                    out.push(GrepMatch {
                        path: rel.to_string_lossy().to_string(),
                        line: (idx as u64) + 1,
                        snippet: line.trim().to_string(),
                    });

                    if out.len() >= max_matches {
                        warnings.push(format!(
                            "match limit reached (max_matches={}); output truncated",
                            max_matches
                        ));
                        break 'walk;
                    }
                }
            }
        }

        Ok(GrepResult {
            canonical_root,
            query: query.to_string(),
            matches: out,
            warnings,
        })
    }

    /// Lightweight “information extraction” pass.
    /// Reads small slices of selected files + greps patterns to extract env vars, URLs, ports, and basic repo facts.
    pub fn extract_repo_facts(&self, root: impl AsRef<str>) -> Result<RepoFacts> {
        let requested_root = root.as_ref().to_string();
        let root = self.resolve_under_base(&requested_root)?;
        let canonical_root = root.display().to_string();

        let digest = self.folder_digest(requested_root)?;
        let mut warnings = digest.warnings.clone();

        let mut env_vars: BTreeSet<String> = BTreeSet::new();
        let mut urls: BTreeSet<String> = BTreeSet::new();
        let mut ports: BTreeSet<u16> = BTreeSet::new();

        let re_env = Regex::new(r"\b([A-Z][A-Z0-9_]{2,})\b").unwrap();
        let re_url = Regex::new(r#"https?://[^\s'\">)]+"#).unwrap();
        let re_port = Regex::new(r"\b(?:PORT|port)\s*[:=]\s*(\d{2,5})\b").unwrap();

        // Focus on samples first
        for s in &digest.samples {
            for cap in re_env.captures_iter(&s.head_text) {
                env_vars.insert(cap[1].to_string());
            }
            for m in re_url.find_iter(&s.head_text) {
                urls.insert(m.as_str().to_string());
            }
            for cap in re_port.captures_iter(&s.head_text) {
                if let Ok(p) = cap[1].parse::<u16>() {
                    ports.insert(p);
                }
            }
        }

        // Detect docker facts
        let mut docker = DockerFacts {
            has_dockerfile: digest
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::File && n.path.eq_ignore_ascii_case("Dockerfile")),
            compose_files: BTreeSet::new(),
        };
        for n in &digest.nodes {
            let lower = n.path.to_lowercase();
            if lower.contains("docker-compose") && matches!(n.kind, NodeKind::File) {
                docker.compose_files.insert(n.path.clone());
            }
        }

        // Rust facts (very light)
        let mut rust = None;
        if digest
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::File && n.path.eq_ignore_ascii_case("Cargo.toml"))
        {
            // Attempt to extract crate names by grepping `name = "..."` in Cargo.toml files
            let mut crates = BTreeSet::new();
            let mut binaries = BTreeSet::new();

            let grep_cargo = self.grep(root.display().to_string(), "name =", true, 2000)?;
            for m in grep_cargo.matches {
                if (m.path.eq_ignore_ascii_case("Cargo.toml") || m.path.ends_with("/Cargo.toml"))
                    && let Some(name) = parse_cargo_name_line(&m.snippet)
                {
                    crates.insert(name);
                }
                if m.path.ends_with("src/main.rs") {
                    binaries.insert("src/main.rs".to_string());
                }
            }

            rust = Some(RustFacts {
                has_cargo_toml: true,
                crates,
                binaries,
            });
        }

        // Extra: scan for `.env` usage without reading .env content
        let grep_env_ref = self.grep(root.display().to_string(), ".env", false, 200)?;
        if !grep_env_ref.matches.is_empty() {
            warnings.push("references to .env found; ensure secrets are not committed".to_string());
        }

        Ok(RepoFacts {
            canonical_root,
            rust,
            docker,
            env_vars,
            urls,
            ports,
            warnings,
        })
    }
}

/* ---------------- helpers ---------------- */

fn default_denies() -> Vec<String> {
    vec![
        ".git/**",
        "target/**",
        "node_modules/**",
        "dist/**",
        "build/**",
        "out/**",
        ".next/**",
        ".venv/**",
        "venv/**",
        "__pycache__/**",
        "*.log",
        "*.tmp",
        "*.swp",
        ".DS_Store",
        ".env",
        ".env.*",
        "*.pem",
        "*.key",
        "*.p12",
        "*.kdbx",
        ".ssh/**",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect()
}

fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        let glob = Glob::new(g).with_context(|| format!("invalid glob: {g}"))?;
        b.add(glob);
    }
    Ok(b.build()?)
}

fn hash_prefix(path: &Path, max_bytes: usize) -> Result<String> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; max_bytes];
    let n = f.read(&mut buf)?;
    buf.truncate(n);

    let mut hasher = Sha256::new();
    hasher.update(&buf);
    let h = hasher.finalize();
    Ok(hex::encode(h)[..8].to_string())
}

fn is_binary_prefix(path: &Path, max_bytes: usize) -> bool {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };
    let mut buf = vec![0u8; max_bytes];
    let n = match f.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return true,
    };
    buf.truncate(n);
    buf.contains(&0)
}

fn detect_signals(files: &BTreeSet<String>) -> Vec<String> {
    let mut signals = Vec::new();

    let has = |name: &str| files.iter().any(|p| p.eq_ignore_ascii_case(name));
    if has("Cargo.toml") {
        signals.push("rust_project: Cargo.toml present".to_string());
    }
    if has("package.json") {
        signals.push("node_project: package.json present".to_string());
    }
    if has("pyproject.toml") || has("requirements.txt") {
        signals.push("python_project: pyproject/requirements present".to_string());
    }
    if has("Dockerfile") {
        signals.push("docker: Dockerfile present".to_string());
    }
    if files
        .iter()
        .any(|p| p.to_lowercase().contains("docker-compose"))
    {
        signals.push("docker_compose: compose file present".to_string());
    }
    if files.iter().any(|p| p.to_lowercase().starts_with("readme")) {
        signals.push("docs: README present".to_string());
    }
    signals
}

fn sample_smart(root: &Path, files: &BTreeSet<String>, max_bytes: usize) -> Vec<FileSample> {
    // Priority targets
    let priority = [
        "README",
        "README.md",
        "README.MD",
        "Cargo.toml",
        "Cargo.lock",
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "Dockerfile",
        "docker-compose.yml",
        "docker-compose.yaml",
        "Makefile",
        "src/main.rs",
        "src/lib.rs",
    ];

    let mut picked: Vec<String> = Vec::new();
    for p in priority {
        if let Some(f) = files.iter().find(|f| f.eq_ignore_ascii_case(p)) {
            picked.push(f.clone());
        }
    }

    // Add up to 25 markdown files
    for f in files.iter() {
        if picked.len() >= 25 {
            break;
        }
        let lower = f.to_lowercase();
        if lower.ends_with(".md") && !picked.contains(f) {
            picked.push(f.clone());
        }
    }

    let mut out = Vec::new();
    for rel in picked.into_iter().take(25) {
        if let Some((truncated, text)) = read_head_text(root.join(&rel), max_bytes) {
            out.push(FileSample {
                path: rel,
                truncated,
                head_text: text,
            });
        }
    }
    out
}

fn read_head_text(path: PathBuf, max_bytes: usize) -> Option<(bool, String)> {
    let mut f = File::open(path).ok()?;
    let mut buf = vec![0u8; max_bytes];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);

    if buf.contains(&0) {
        return None;
    }
    let s = std::str::from_utf8(&buf).ok()?;
    Some((n == max_bytes, s.to_string()))
}

fn parse_cargo_name_line(line: &str) -> Option<String> {
    // naive parse: name = "crate_name"
    let line = line.trim();
    if !line.starts_with("name") {
        return None;
    }
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() != 2 {
        return None;
    }
    let rhs = parts[1].trim();
    let rhs = rhs.trim_matches('"').trim_matches('\'');
    if rhs.is_empty() {
        None
    } else {
        Some(rhs.to_string())
    }
}
