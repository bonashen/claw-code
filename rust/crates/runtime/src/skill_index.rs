use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// BM25 parameters.
const K1: f64 = 1.5;   // term saturation (1.2–2.0)
const B: f64 = 0.75;  // length normalisation (0.0–1.0)

pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub path: PathBuf,
}

/// A skill root directory with its last-known modification time.
/// Used to detect when skills are installed, modified, or deleted.
struct MonitoredRoot {
    path: PathBuf,
    latest_mtime: SystemTime,
}

/// BM25-indexed skill store.
pub struct SkillIndex {
    skills: Vec<SkillMeta>,
    /// term → [(doc_idx, term_frequency_in_doc)]
    term_index: HashMap<String, Vec<(usize, usize)>>,
    /// term → number of documents containing it
    doc_freq: HashMap<String, usize>,
    /// total terms per document
    doc_lengths: Vec<usize>,
    avg_doc_len: f64,
    num_docs: usize,
}

/// Split text into normalised indexable tokens (lowercased, >2 chars).
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect()
}

impl SkillIndex {
    pub fn new(skills: Vec<SkillMeta>) -> Self {
        let num_docs = skills.len();
        let mut term_index: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let mut doc_lengths: Vec<usize> = Vec::with_capacity(num_docs);

        for (idx, skill) in skills.iter().enumerate() {
            let mut tokens: Vec<String> = Vec::new();
            tokens.extend(tokenize(&skill.name));
            tokens.extend(tokenize(&skill.description));
            for kw in &skill.keywords {
                tokens.extend(tokenize(kw));
            }

            doc_lengths.push(tokens.len());

            let mut tf: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *tf.entry(token).or_default() += 1;
            }

            for (term, freq) in tf {
                term_index.entry(term).or_default().push((idx, freq));
            }
        }

        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for (term, postings) in &term_index {
            doc_freq.insert(term.clone(), postings.len());
        }

        // average document length
        let avg_doc_len = if num_docs > 0 {
            let total: usize = doc_lengths.iter().sum();
            total as f64 / num_docs as f64
        } else {
            0.0
        };

        Self {
            skills,
            term_index,
            doc_freq,
            doc_lengths,
            avg_doc_len,
            num_docs,
        }
    }

    /// BM25 retrieval score for a single (term, doc).
    fn bm25_score(&self, tf: usize, doc_len: usize, n: usize) -> f64 {
        let idf = ((self.num_docs as f64 - n as f64 + 0.5) / (n as f64 + 0.5) + 1.0).ln();
        let norm = 1.0 - B + B * doc_len as f64 / self.avg_doc_len;
        let term_saturation = tf as f64 * (K1 + 1.0) / (tf as f64 + K1 * norm);
        idf * term_saturation
    }

    pub fn retrieve(&self, query: &str, top_k: usize) -> Vec<SkillSummary> {
        let query_words = tokenize(query);

        if query_words.is_empty() {
            return self
                .skills
                .iter()
                .take(top_k)
                .map(|s| SkillSummary {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    keywords: s.keywords.clone(),
                })
                .collect();
        }

        let mut scores: HashMap<usize, f64> = HashMap::new();

        for word in &query_words {
            let n = self.doc_freq.get(word).copied().unwrap_or(0);
            if n == 0 {
                continue;
            }
            if let Some(postings) = self.term_index.get(word) {
                for &(doc_idx, tf) in postings {
                    let doc_len = self.doc_lengths[doc_idx];
                    let score = self.bm25_score(tf, doc_len, n);
                    *scores.entry(doc_idx).or_insert(0.0) += score;
                }
            }
        }

        let mut sorted: Vec<(usize, f64)> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        sorted
            .into_iter()
            .take(top_k)
            .filter_map(|(idx, _)| self.skills.get(idx))
            .map(|s| SkillSummary {
                name: s.name.clone(),
                description: s.description.clone(),
                keywords: s.keywords.clone(),
            })
            .collect()
    }

    pub fn get_skill_path(&self, name: &str) -> Option<PathBuf> {
        self.skills
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.path.clone())
    }

    pub fn skill_names(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.name.clone()).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
}

impl SkillSummary {
    pub fn to_index_line(&self) -> String {
        format!("- {}: {}", self.name, self.description)
    }
}

pub struct GlobalSkillIndex {
    inner: Mutex<Option<SkillIndex>>,
    /// Monitored roots for change detection.
    roots: Mutex<Vec<MonitoredRoot>>,
}

impl GlobalSkillIndex {
    pub const fn new() -> Self {
        GlobalSkillIndex {
            inner: Mutex::new(None),
            roots: Mutex::new(Vec::new()),
        }
    }

    /// Initialise or refresh the index if any skill root has changed.
    pub fn ensure_fresh(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh_inner(&mut guard);
    }

    /// Refresh: compare stored mtimes with current filesystem.
    /// Returns true if the index was rebuilt.
    fn refresh_inner(&self, guard: &mut Option<SkillIndex>) -> bool {
        if self.any_root_changed() || guard.is_none() {
            let (skills, fresh_roots) = discover_all_skills();
            let mut root_guard = self.roots.lock().unwrap_or_else(|e| e.into_inner());
            *root_guard = fresh_roots;
            *guard = Some(SkillIndex::new(skills));
            true
        } else {
            false
        }
    }

    /// Check if any monitored root directory has been modified.
    /// Uses the same child-scanning logic as `root_observed_mtime`.
    fn any_root_changed(&self) -> bool {
        let root_guard = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        for root in root_guard.iter() {
            match root_observed_mtime(&root.path) {
                Some(m) if m > root.latest_mtime => return true,
                None => return true, // directory disappeared
                _ => {}
            }
        }
        false
    }

    pub fn retrieve(&self, query: &str, top_k: usize) -> Vec<SkillSummary> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh_inner(&mut guard);
        guard
            .as_ref()
            .map(|idx| idx.retrieve(query, top_k))
            .unwrap_or_default()
    }

    pub fn get_skill_path(&self, name: &str) -> Option<PathBuf> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh_inner(&mut guard);
        guard.as_ref().and_then(|idx| idx.get_skill_path(name))
    }
}

static SKILL_INDEX: GlobalSkillIndex = GlobalSkillIndex::new();

/// Returns the number of discovered and indexed skills.
/// Idempotent; safe to call multiple times.
pub fn init_skill_index() -> usize {
    SKILL_INDEX.ensure_fresh();
    let guard = SKILL_INDEX.inner.lock().unwrap();
    guard.as_ref().map(|i| i.skill_names().len()).unwrap_or(0)
}

/// Return the mtime of a directory, or the latest mtime of its entries.
fn fs_mtime(path: &Path) -> Result<SystemTime, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    meta.modified()
}

/// Compute the "observed mtime" for a root: the latest mtime among the root
/// directory itself and its immediate skill-subdirectory entries.
fn root_observed_mtime(root: &Path) -> Option<SystemTime> {
    let root_mtime = fs_mtime(root).ok()?;
    let mut latest = root_mtime;

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Ok(m) = entry.metadata().and_then(|m| m.modified()) {
                    if m > latest {
                        latest = m;
                    }
                }
            }
        }
    }

    Some(latest)
}

/// Scrape all skill directories from roots and return flat skill list.
fn scan_root_skills(root: &Path) -> Vec<SkillMeta> {
    let mut skills = Vec::new();
    if !root.is_dir() {
        return skills;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy().to_string();
                let skill_path = entry.path();
                if let Some(meta) = extract_skill_metadata(&name, &skill_path) {
                    skills.push(meta);
                }
            }
        }
    }
    skills
}

fn push_root(roots: &mut Vec<MonitoredRoot>, path: PathBuf) {
    if path.is_dir() {
        if let Some(mtime) = root_observed_mtime(&path) {
            roots.push(MonitoredRoot { path, latest_mtime: mtime });
        }
    }
}

/// Enumerate all skill root directories (project‑local, user, config).
/// Mirrors the lookup order in tools/src/lib.rs `skill_lookup_roots()`.
fn discover_all_skill_roots() -> Vec<MonitoredRoot> {
    let mut roots: Vec<MonitoredRoot> = Vec::new();

    let cwd = std::env::current_dir().ok();

    // 1. Project-local roots (walk ancestors)
    if let Some(ref cwd) = cwd {
        let mut ancestor: Option<&Path> = Some(cwd.as_path());
        while let Some(dir) = ancestor {
            add_prefixed_skills_root(&mut roots, &dir.join(".omc"));
            add_prefixed_skills_root(&mut roots, &dir.join(".agents"));
            add_prefixed_skills_root(&mut roots, &dir.join(".claw"));
            add_prefixed_skills_root(&mut roots, &dir.join(".codex"));
            add_prefixed_skills_root(&mut roots, &dir.join(".claude"));
            ancestor = dir.parent();
        }
    }

    // 2. Config home roots
    if let Ok(home) = std::env::var("CLAW_CONFIG_HOME") {
        push_root(&mut roots, Path::new(&home).join("skills"));
    }
    if let Ok(home) = std::env::var("CODEX_HOME") {
        push_root(&mut roots, Path::new(&home).join("skills"));
    }

    // 3. User home roots
    if let Ok(home) = std::env::var("HOME").or_else(|_| {
        // Windows: try USERPROFILE then HOMEDRIVE+HOMEPATH
        std::env::var("USERPROFILE")
    }) {
        let home = PathBuf::from(home);
        add_prefixed_skills_root(&mut roots, &home.join(".omc"));
        add_prefixed_skills_root(&mut roots, &home.join(".claw"));
        add_prefixed_skills_root(&mut roots, &home.join(".codex"));
        add_prefixed_skills_root(&mut roots, &home.join(".claude"));

        push_root(&mut roots, home.join(".agents").join("skills"));
        push_root(&mut roots, home.join(".config").join("opencode").join("skills"));
        push_root(&mut roots, home.join(".claude").join("skills").join("omc-learned"));
    }

    roots
}

fn add_prefixed_skills_root(roots: &mut Vec<MonitoredRoot>, prefix: &Path) {
    push_root(roots, prefix.join("skills"));
    push_root(roots, prefix.join("commands"));
}

/// Discover all skills across ALL root directories.
fn discover_all_skills() -> (Vec<SkillMeta>, Vec<MonitoredRoot>) {
    let roots = discover_all_skill_roots();
    let mut skills: Vec<SkillMeta> = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for root in &roots {
        let root_skills = scan_root_skills(&root.path);
        for skill in root_skills {
            // Deduplicate by name: first root wins (project-local > user > config)
            if seen_names.insert(skill.name.clone()) {
                skills.push(skill);
            }
        }
    }

    (skills, roots)
}

fn extract_skill_metadata(name: &str, path: &std::path::Path) -> Option<SkillMeta> {
    let md_path = path.join("SKILL.md");
    let content = if md_path.is_file() {
        std::fs::read_to_string(&md_path).ok()?
    } else {
        String::new()
    };

    let description = extract_description(&content);
    let keywords = extract_keywords(&content);

    Some(SkillMeta {
        name: name.to_string(),
        description,
        keywords,
        path: path.to_path_buf(),
    })
}

fn extract_description(content: &str) -> String {
    // Try to parse frontmatter description field first
    if let Some(frontmatter) = parse_frontmatter(content) {
        for line in frontmatter.lines() {
            if let Some(value) = line.strip_prefix("description:")
                .or_else(|| line.strip_prefix("description :"))
            {
                let desc = value.trim();
                if !desc.is_empty() {
                    if desc.chars().count() <= 100 {
                        return desc.to_string();
                    }
                    return format!("{}...", desc.chars().take(97).collect::<String>());
                }
            }
        }
    }

    // Fallback: first meaningful non-header line after frontmatter
    let body = content
        .strip_prefix("---")
        .and_then(|c| c.find("---"))
        .map(|end| &content[end + 6..])
        .unwrap_or(content);

    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            if trimmed.chars().count() <= 100 {
                return trimmed.to_string();
            }
            return format!("{}...", trimmed.chars().take(97).collect::<String>());
        }
    }
    String::new()
}

/// Extract the raw frontmatter body (text between first and second `---`).
fn parse_frontmatter(content: &str) -> Option<&str> {
    let after_first = content.strip_prefix("---")?;
    let end = after_first.find("---")?;
    Some(&after_first[..end])
}

fn extract_keywords(content: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    let lower = content.to_lowercase();

    let important = ["api", "config", "setup", "install", "deploy", "test", "build", "debug", "git", "docker", "kubernetes", "aws", "cloud", "linux", "windows", "shell", "script", "automation", "ci", "cd", "monitoring", "logging", "security", "database", "sql", "redis", "cache"];

    for kw in important {
        if lower.contains(kw) {
            keywords.push(kw.to_string());
        }
    }

    keywords
}

pub fn generate_skill_index_prompt(query: &str, max_skills: usize) -> String {
    let skills = SKILL_INDEX.retrieve(query, max_skills);
    build_skill_section(&skills)
}

/// Build the markdown prompt section from a list of skill summaries.
/// Produces e.g.:
///   ## Available Skills
///   - name: description
///
///   Use skill_view(name="skill-name") to load a skill's full content when needed.
pub fn build_skill_section(skills: &[SkillSummary]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = skills.iter().map(|s| s.to_index_line()).collect();

    format!(
        "## Available Skills\n{}\n\nUse skill_view(name=\"skill-name\") to load a skill's full content when needed.\n",
        lines.join("\n")
    )
}

pub fn get_skill_path_for_loading(name: &str) -> Option<PathBuf> {
    SKILL_INDEX.get_skill_path(name)
}

fn load_skill_content_cached(name: &str) -> Option<String> {
    static CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<String, String>>> = 
        std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
    
    {
        let guard = CACHE.lock().unwrap();
        if let Some(content) = guard.get(name) {
            return Some(content.clone());
        }
    }
    
    if let Some(path) = get_skill_path_for_loading(name) {
        let md_path = path.join("SKILL.md");
        if let Ok(content) = std::fs::read_to_string(&md_path) {
            let mut guard = CACHE.lock().unwrap();
            guard.insert(name.to_string(), content.clone());
            return Some(content);
        }
    }
    
    None
}

pub fn get_skill_for_tool(name: &str) -> Option<String> {
    load_skill_content_cached(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_keyword_match() {
        let skills = vec![
            SkillMeta {
                name: "github-pr-workflow".to_string(),
                description: "Automated PR creation and review workflow".to_string(),
                keywords: vec!["git".to_string(), "github".to_string(), "pr".to_string()],
                path: PathBuf::from("/skills/github-pr-workflow"),
            },
            SkillMeta {
                name: "docker-deployment".to_string(),
                description: "Deploy applications using Docker containers".to_string(),
                keywords: vec!["docker".to_string(), "containers".to_string(), "deploy".to_string()],
                path: PathBuf::from("/skills/docker-deployment"),
            },
        ];

        let index = SkillIndex::new(skills);

        let results = index.retrieve("docker container deploy", 5);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name == "docker-deployment"));
    }

    #[test]
    fn test_bm25_idf_rare_term_ranks_higher() {
        // Both skills mention "deploy", but only one has the rarer "kubernetes"
        let skills = vec![
            SkillMeta {
                name: "generic-deploy".to_string(),
                description: "Deploy applications".to_string(),
                keywords: vec!["deploy".to_string()],
                path: PathBuf::from("/skills/generic-deploy"),
            },
            SkillMeta {
                name: "k8s-deploy".to_string(),
                description: "Deploy on Kubernetes clusters".to_string(),
                keywords: vec!["kubernetes".to_string(), "deploy".to_string()],
                path: PathBuf::from("/skills/k8s-deploy"),
            },
            SkillMeta {
                name: "docker-deploy".to_string(),
                description: "Deploy with Docker".to_string(),
                keywords: vec!["docker".to_string(), "deploy".to_string()],
                path: PathBuf::from("/skills/docker-deploy"),
            },
        ];

        let index = SkillIndex::new(skills);

        // Query "kubernetes deploy" — k8s-deploy should rank first
        // because "kubernetes" has higher IDF (appears in only 1 of 3 docs)
        let results = index.retrieve("kubernetes deploy", 3);
        assert_eq!(results[0].name, "k8s-deploy", "rare term should boost rank");
    }

    #[test]
    fn test_bm25_multiple_terms_boost() {
        let skills = vec![
            SkillMeta {
                name: "git-workflow".to_string(),
                description: "Basic Git version control".to_string(),
                keywords: vec!["git".to_string()],
                path: PathBuf::from("/skills/git-workflow"),
            },
            SkillMeta {
                name: "github-actions".to_string(),
                description: "GitHub Actions CI/CD pipelines".to_string(),
                keywords: vec!["git".to_string(), "github".to_string(), "ci".to_string()],
                path: PathBuf::from("/skills/github-actions"),
            },
        ];

        let index = SkillIndex::new(skills);

        // Query "github ci" matches only github-actions
        let results = index.retrieve("github ci", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "github-actions");
    }

    #[test]
    fn test_bm25_empty_query_returns_first_n() {
        let skills = vec![
            SkillMeta {
                name: "skill-a".to_string(),
                description: "First skill".to_string(),
                keywords: vec![],
                path: PathBuf::from("/skills/a"),
            },
            SkillMeta {
                name: "skill-b".to_string(),
                description: "Second skill".to_string(),
                keywords: vec![],
                path: PathBuf::from("/skills/b"),
            },
        ];

        let index = SkillIndex::new(skills);
        let results = index.retrieve("", 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "skill-a");
    }

    #[test]
    fn test_filesystem_discover_and_retrieve() {
        use std::fs;

        let tmp = std::env::temp_dir().join(format!("skill_test_{}", std::process::id()));
        let skills_dir = tmp.join(".claude").join("skills");

        let go_dir = skills_dir.join("golang-deploy");
        fs::create_dir_all(&go_dir).unwrap();
        fs::write(
            go_dir.join("SKILL.md"),
            "---\nname: golang-deploy\ndescription: Deploy Go applications to production\n---\n\nUse this skill when deploying Go services. It covers building binaries, containerizing with Docker, and rolling out to Kubernetes clusters."
        ).unwrap();

        let py_dir = skills_dir.join("python-testing");
        fs::create_dir_all(&py_dir).unwrap();
        fs::write(
            py_dir.join("SKILL.md"),
            "---\nname: python-testing\ndescription: Python test suite with pytest\n---\n\nCovers unit tests, integration tests, and coverage reporting using pytest."
        ).unwrap();

        // Use the public end-to-end API
        let results = {
            let original = std::env::current_dir().ok();
            std::env::set_current_dir(&tmp).unwrap();
            let count = super::init_skill_index();
            let r = super::SKILL_INDEX.retrieve("docker kubernetes deploy", 3);
            if let Some(dir) = original {
                let _ = std::env::set_current_dir(dir);
            }
            (count, r)
        };

        let _ = fs::remove_dir_all(&tmp);

        assert!(
            results.0 >= 2,
            "init_skill_index should find at least 2 skills, got {}",
            results.0
        );
        assert!(!results.1.is_empty());
        assert_eq!(results.1[0].name, "golang-deploy",
            "golang-deploy should rank first for 'docker kubernetes deploy'");
    }

    #[test]
    fn test_filesystem_mtime_triggers_refresh() {
        use std::fs;

        // Create temp dir with a project-level .claw skill
        let tmp_a = std::env::temp_dir().join(format!("skill_mtime_a_{}", std::process::id()));
        let skills_a = tmp_a.join(".claw").join("skills");
        let skill_a_dir = skills_a.join("skill-a");
        fs::create_dir_all(&skill_a_dir).unwrap();
        fs::write(skill_a_dir.join("SKILL.md"), "# Skill A\ntesting skill A content").unwrap();

        // Create a second temp dir with different .claw skills
        let tmp_b = std::env::temp_dir().join(format!("skill_mtime_b_{}", std::process::id()));
        let skills_b = tmp_b.join(".claw").join("skills");
        let skill_b_dir = skills_b.join("skill-b");
        fs::create_dir_all(&skill_b_dir).unwrap();
        fs::write(skill_b_dir.join("SKILL.md"), "# Skill B\ntesting skill B content").unwrap();

        // Switch to tmp_a, init index — should pick up skill-a
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(&tmp_a).unwrap();
        super::init_skill_index();

        let results_a = super::SKILL_INDEX.retrieve("testing", 100);
        let names_a: Vec<&str> = results_a.iter().map(|s| s.name.as_str()).collect();
        assert!(names_a.contains(&"skill-a"), "skill-a should be found in dir A");

        // Delete tmp_a to force root staleness, then switch to tmp_b and re-init
        // This simulates skills changing on disk: the old root disappears,
        // and the new CWD has a different skill set.
        let _ = fs::remove_dir_all(&tmp_a);
        std::env::set_current_dir(&tmp_b).unwrap();
        super::init_skill_index();

        let results_b = super::SKILL_INDEX.retrieve("testing", 100);
        let names_b: Vec<&str> = results_b.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names_b.contains(&"skill-b"),
            "after switching CWD, skill-b should be found. Got: {:?}",
            names_b
        );

        if let Some(dir) = original {
            let _ = std::env::set_current_dir(dir);
        }
        let _ = fs::remove_dir_all(&tmp_b);
    }

    #[test]
    fn test_bm25_no_match_returns_empty() {
        let skills = vec![
            SkillMeta {
                name: "rust-linting".to_string(),
                description: "Rust code linting with Clippy".to_string(),
                keywords: vec!["rust".to_string()],
                path: PathBuf::from("/skills/rust-linting"),
            },
        ];

        let index = SkillIndex::new(skills);
        let results = index.retrieve("quantum physics", 5);
        assert!(results.is_empty());
    }

    // ── Prompt output tests ──────────────────────────────────────────

    #[test]
    fn test_to_index_line_format() {
        let summary = SkillSummary {
            name: "docker-deploy".to_string(),
            description: "Deploy with Docker containers".to_string(),
            keywords: vec!["docker".to_string()],
        };
        let line = summary.to_index_line();
        assert_eq!(line, "- docker-deploy: Deploy with Docker containers");
    }

    #[test]
    fn test_generate_skill_index_prompt_format() {
        let skills = vec![
            SkillMeta {
                name: "docker-deploy".to_string(),
                description: "Deploy with Docker containers".to_string(),
                keywords: vec!["docker".to_string()],
                path: PathBuf::from("/skills/docker-deploy"),
            },
            SkillMeta {
                name: "git-workflow".to_string(),
                description: "Git version control workflow".to_string(),
                keywords: vec!["git".to_string()],
                path: PathBuf::from("/skills/git-workflow"),
            },
        ];

        let index = SkillIndex::new(skills);
        let summaries = index.retrieve("docker", 5);
        let prompt = super::build_skill_section(&summaries);

        // Must start with section header
        assert!(prompt.starts_with("## Available Skills"), "prompt must start with header");

        // Must contain the matched skill line
        assert!(prompt.contains("- docker-deploy: Deploy with Docker containers"));

        // Must NOT contain the unmatched skill
        assert!(!prompt.contains("git-workflow"),
            "only matched skills should appear in prompt");

        // Must include usage instruction
        assert!(prompt.contains("skill_view"), "prompt must explain how to load skill");
    }

    #[test]
    fn test_generate_skill_index_prompt_empty() {
        let prompt = super::build_skill_section(&[]);
        assert!(prompt.is_empty(), "empty skills should produce empty prompt");
    }

    #[test]
    fn test_generate_skill_index_prompt_top_k_respected() {
        let skills = vec![
            SkillMeta {
                name: "skill-a".to_string(),
                description: "Alpha test skill".to_string(),
                keywords: vec!["test".to_string()],
                path: PathBuf::from("/skills/a"),
            },
            SkillMeta {
                name: "skill-b".to_string(),
                description: "Bravo test skill".to_string(),
                keywords: vec!["test".to_string()],
                path: PathBuf::from("/skills/b"),
            },
            SkillMeta {
                name: "skill-c".to_string(),
                description: "Charlie test skill".to_string(),
                keywords: vec!["test".to_string()],
                path: PathBuf::from("/skills/c"),
            },
        ];

        let index = SkillIndex::new(skills);
        let summaries = index.retrieve("test", 2);
        assert_eq!(summaries.len(), 2, "top_k=2 should return exactly 2 results");

        let prompt = super::build_skill_section(&summaries);
        let line_count = prompt.lines()
            .filter(|l| l.starts_with("- "))
            .count();
        assert_eq!(line_count, 2, "prompt must contain exactly 2 bullet lines");
    }

    #[test]
    fn test_filesystem_prompt_roundtrip() {
        use std::fs;

        let tmp = std::env::temp_dir().join(format!("skill_prompt_test_{}", std::process::id()));
        let skills_dir = tmp.join(".claude").join("skills");

        let dirs = ["golang-deploy", "python-testing"];
        let contents = [
            "---\ndescription: Deploy Go applications to production\n---\n\nUse Docker and Kubernetes for Go services.",
            "---\ndescription: Python test suite with pytest\n---\n\nRun unit tests and integration tests.",
        ];

        for (i, name) in dirs.iter().enumerate() {
            let d = skills_dir.join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), contents[i]).unwrap();
        }

        let original = std::env::current_dir().ok();
        std::env::set_current_dir(&tmp).unwrap();

        // Init index from this CWD
        let count = super::init_skill_index();
        assert!(count >= 2, "should index at least 2 skills, got {count}");

        // Verify raw retrieve works before testing the prompt wrapper
        let raw = super::SKILL_INDEX.retrieve("python pytest", 5);
        assert!(!raw.is_empty(), "raw BM25 retrieve should find python-testing");
        let raw_names: Vec<&str> = raw.iter().map(|s| s.name.as_str()).collect();
        assert!(
            raw_names.contains(&"python-testing"),
            "python-testing must be in raw results, got: {:?}",
            raw_names
        );

        // Now test the prompt-level wrapper
        let result = super::generate_skill_index_prompt("python pytest", 5);
        assert!(!result.is_empty(), "should produce non-empty prompt");
        assert!(result.starts_with("## Available Skills"), "must start with header");
        assert!(result.contains("skill_view"), "must include usage instruction");
        assert!(result.contains("python-testing"),
            "python-testing should match 'python pytest' query");

        if let Some(dir) = original {
            let _ = std::env::set_current_dir(dir);
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}