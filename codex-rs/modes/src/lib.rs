pub use indexmap::IndexMap;
pub use indexmap::IndexSet;
use regex::Regex;
use serde::Deserialize;
pub use serde_yaml::Value as YamlValue;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeKind {
    Persistent,
    Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VarType {
    Text,
    Enum,
    Boolean,
    Number,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeScope {
    Project(String),
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ShortcutSpec {
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub key_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModeVariableDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: Option<VarType>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
    #[serde(default)]
    pub r#enum: Option<Vec<String>>,
    #[serde(default)]
    pub shortcuts: Option<Vec<String>>, // simplified for now: ["-t", "ticket="]
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub inline_edit: Option<bool>,
    #[serde(default)]
    pub mode_scoped: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeDefinition {
    pub id: String, // "/a:b:c"
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub kind: ModeKind,
    pub default_enabled: bool,
    pub variables: Vec<ModeVariableDefinition>,
    pub scope: ModeScope,
    pub path: PathBuf,
    pub body: String,
}

#[derive(Debug, Error)]
pub enum ModesError {
    #[error("illegal id: {0}")]
    IllegalId(String), // E1001
    #[error("io error: {0}")]
    Io(String), // E1004
    #[error("parse frontmatter: {0}")]
    Frontmatter(String), // E2001
    #[error("duplicate variable: {0}")]
    VarDup(String), // E2101
    #[error("bad regex: {0}")]
    Regex(String), // E2201
    #[error("unknown mode: {0}")]
    UnknownMode(String), // E1201
}

#[derive(Debug, Deserialize)]
struct FrontmatterRaw {
    #[serde(default)]
    kind: Option<ModeKind>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    argument_hint: Option<String>,
    #[serde(default)]
    default_enabled: Option<bool>,
    #[serde(default)]
    variables: Option<Vec<ModeVariableDefinition>>,
}

/// Return `(frontmatter, body)` if frontmatter block exists; otherwise treat whole file as body.
fn parse_frontmatter(text: &str) -> Result<(Option<FrontmatterRaw>, String), ModesError> {
    let s = text;
    if let Some(rest) = s.strip_prefix("---\n")
        && let Some(idx) = rest.find("\n---\n")
    {
        let (yaml, body) = rest.split_at(idx);
        let body = &body[5..]; // skip "\n---\n"
        let fm: FrontmatterRaw =
            serde_yaml::from_str(yaml).map_err(|e| ModesError::Frontmatter(e.to_string()))?;
        return Ok((Some(fm), body.to_string()));
    }
    Ok((None, s.to_string()))
}

fn sanitize_path_segment(seg: &str) -> Option<String> {
    let ok = seg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok && !seg.is_empty() {
        Some(seg.to_string())
    } else {
        None
    }
}

fn id_from_rel_path(rel: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for comp in rel.iter() {
        let os = comp.to_string_lossy();
        let no_ext = os.split('.').next().unwrap_or("");
        let trimmed = no_ext.trim();
        if trimmed.is_empty() {
            return None;
        }
        let safe = sanitize_path_segment(trimmed)?;
        parts.push(safe);
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("/{}", parts.join(":")))
}

fn project_chain_from_repo_root_to(cwd: &Path) -> Vec<PathBuf> {
    // Walk up to find git root
    let mut chain: Vec<PathBuf> = vec![cwd.to_path_buf()];
    let mut cursor = cwd.to_path_buf();
    let mut git_root: Option<PathBuf> = None;
    while let Some(parent) = cursor.parent() {
        let marker = cursor.join(".git");
        if marker.is_file() || marker.is_dir() {
            git_root = Some(cursor.clone());
            break;
        }
        chain.push(parent.to_path_buf());
        cursor = parent.to_path_buf();
    }
    if let Some(root) = git_root {
        let mut out = Vec::new();
        let mut saw_root = false;
        for p in chain.iter().rev() {
            if !saw_root {
                if *p == root {
                    saw_root = true;
                } else {
                    continue;
                }
            }
            out.push(p.clone());
        }
        out
    } else {
        vec![cwd.to_path_buf()]
    }
}

/// Scan .codex/modes along repo-root→cwd and append $CODEX_HOME/modes if present.
pub fn scan_modes(
    cwd: &Path,
    codex_home: Option<&Path>,
) -> Result<Vec<ModeDefinition>, ModesError> {
    let mut search_dirs = project_chain_from_repo_root_to(cwd);
    if let Some(home) = codex_home {
        let home_modes = home.join("modes");
        if home_modes.is_dir() {
            search_dirs.push(home_modes);
        }
    }

    let mut seen: IndexMap<String, ModeDefinition> = IndexMap::new();
    for dir in search_dirs {
        let modes_dir = dir.join(".codex/modes");
        if !modes_dir.is_dir() {
            continue;
        }
        let scope = if codex_home.map(|h| h.join("modes")).as_ref() == Some(&modes_dir) {
            ModeScope::Global
        } else {
            let label = dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            ModeScope::Project(label)
        };
        let walk = walkdir::WalkDir::new(&modes_dir).into_iter();
        for entry in walk.filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let rel = match entry.path().strip_prefix(&modes_dir) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Some(id) = id_from_rel_path(rel) else {
                continue;
            }; // illegal id skipped
            let text =
                fs::read_to_string(entry.path()).map_err(|e| ModesError::Io(e.to_string()))?;
            let (fm, body) = parse_frontmatter(&text)?;
            let (kind, display_name, description, argument_hint, default_enabled, variables) =
                if let Some(fm) = fm {
                    (
                        fm.kind.unwrap_or(ModeKind::Persistent),
                        fm.display_name,
                        fm.description,
                        fm.argument_hint,
                        fm.default_enabled.unwrap_or(false),
                        fm.variables.unwrap_or_default(),
                    )
                } else {
                    (ModeKind::Persistent, None, None, None, false, Vec::new())
                };

            // validate duplicate vars & regex
            let mut names = std::collections::HashSet::new();
            for v in &variables {
                if !names.insert(v.name.clone()) {
                    return Err(ModesError::VarDup(v.name.clone()));
                }
                if let Some(pat) = &v.pattern {
                    Regex::new(pat).map_err(|e| ModesError::Regex(e.to_string()))?;
                }
            }

            let def = ModeDefinition {
                id: id.clone(),
                display_name,
                description,
                argument_hint,
                kind,
                default_enabled,
                variables,
                scope: scope.clone(),
                path: entry.path().to_path_buf(),
                body,
            };
            seen.insert(id, def); // later write overrides earlier
        }
    }
    Ok(seen.into_values().collect())
}

pub struct EnabledMode<'a> {
    pub id: &'a str,
    pub display_name: Option<&'a str>,
    pub scope: &'a ModeScope,
    pub variables: IndexMap<&'a str, Option<String>>, // None = UseDefault
}

fn coerce_value(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn render_one(
    def: &ModeDefinition,
    vars: &IndexMap<&str, Option<String>>,
) -> (String, String, String) {
    // scope label
    let scope_label = match &def.scope {
        ModeScope::Global => "global".to_string(),
        ModeScope::Project(d) => format!("project:{d}"),
    };
    // variables
    let mut kvs: Vec<String> = Vec::new();
    for v in &def.variables {
        let name = v.name.as_str();
        let val = vars
            .get(name)
            .and_then(|o| o.clone())
            .or_else(|| v.default.as_ref().and_then(coerce_value));
        if let Some(val) = val {
            kvs.push(format!("{name}={val}"));
        }
    }
    let vars_line = kvs.join(", ");

    // template substitute
    let mut rendered = def.body.clone();
    for v in &def.variables {
        let name = v.name.as_str();
        let val = vars
            .get(name)
            .and_then(|o| o.clone())
            .or_else(|| v.default.as_ref().and_then(coerce_value))
            .unwrap_or_default();
        let placeholder = format!("{{{{{name}}}}}");
        rendered = rendered.replace(&placeholder, &val);
    }
    (scope_label, vars_line, rendered)
}

pub fn render_user_instructions(
    base_user_instructions: &str,
    enabled: &[EnabledMode<'_>],
    defs: &[ModeDefinition],
) -> Result<String, ModesError> {
    if enabled.is_empty() {
        return Ok(format!(
            "<user_instructions>\n\n{}\n\n</user_instructions>",
            base_user_instructions.trim()
        ));
    }
    let mut out = String::new();
    use std::fmt::Write;
    writeln!(out, "<user_instructions>\n").ok();
    writeln!(out, "{}\n", base_user_instructions.trim()).ok();
    writeln!(out, "<mode_instructions>").ok();
    for em in enabled {
        let def = defs
            .iter()
            .find(|d| d.id == em.id)
            .ok_or_else(|| ModesError::UnknownMode(em.id.to_string()))?;
        let display = em
            .display_name
            .or(def.display_name.as_deref())
            .unwrap_or_else(|| def.id.trim_start_matches('/'));
        let (scope, vars_line, rendered) = render_one(def, &em.variables);
        writeln!(out, "### Mode: {display}").ok();
        writeln!(out, "- scope: {scope}").ok();
        if !vars_line.is_empty() {
            writeln!(out, "- variables: {vars_line}\n").ok();
        } else {
            writeln!(out).ok();
        }
        if !rendered.trim().is_empty() {
            writeln!(out, "{rendered}\n").ok();
        } else {
            writeln!(out).ok();
        }
    }
    writeln!(out, "</mode_instructions>\n").ok();
    writeln!(out, "</user_instructions>").ok();
    Ok(out)
}

/// Byte-for-byte equality; callers may add a relaxed mode if needed later.
/// Normalize a string for relaxed equivalence comparisons:
/// - Convert CRLF to LF
/// - Trim trailing whitespace on each line
/// - Collapse consecutive blank lines to a single blank line
pub fn normalize_equiv(s: &str) -> String {
    let s = s.replace("\r\n", "\n");
    let mut out = String::new();
    let mut prev_blank = false;
    for line in s.lines() {
        let line = line.trim_end();
        let is_blank = line.is_empty();
        if is_blank {
            if prev_blank {
                continue;
            }
            prev_blank = true;
        } else {
            prev_blank = false;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    // Trim any trailing blank lines/newlines to avoid spurious diffs.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Relaxed equivalence using `normalize_equiv`.
pub fn is_equivalent(a: &str, b: &str) -> bool {
    normalize_equiv(a) == normalize_equiv(b)
}

/// Map a ModesError to a user-facing error message with a stable error code.
/// Conservative default maps to E3201 TemplateError to preserve prior behavior.
pub fn format_modes_error(err: &ModesError) -> String {
    match err {
        ModesError::IllegalId(msg) => format!("E1001 IllegalId: {msg}"),
        ModesError::Io(msg) => format!("E1004 Io: {msg}"),
        ModesError::Frontmatter(msg) => format!("E2001 Frontmatter: {msg}"),
        ModesError::VarDup(var) => format!("E2101 VarDup: {var}"),
        ModesError::Regex(msg) => format!("E2201 Regex: {msg}"),
        ModesError::UnknownMode(id) => format!("E1201 UnknownMode: {id}"),
    }
}

/// Build a compact labels string from the enabled modes, e.g., "a · b · c".
pub fn enabled_labels(enabled: &[EnabledMode<'_>]) -> String {
    enabled
        .iter()
        .map(|e| e.id.trim_start_matches('/'))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Build the standard "Applied N mode(s)" message.
pub fn applied_message(count: usize) -> String {
    format!("Applied {} mode(s)", count)
}

/// Format persistent mode summary from labels; returns None when labels empty.
pub fn format_mode_summary(labels: &str) -> Option<String> {
    let t = labels.trim();
    if t.is_empty() {
        None
    } else {
        Some(format!("Mode: {t}"))
    }
}

/// Lightweight generation-based debouncer helper. Each `next()` call returns a new
/// monotonically increasing generation id. A task scheduled with this id should check
/// `is_latest(id)` before committing side-effects; if it returns false, the task is
/// superseded by a newer change and should no-op.
#[derive(Clone, Default)]
pub struct DebounceGen(Arc<AtomicU64>);

impl DebounceGen {
    pub fn new() -> Self {
        Self::default()
    }
    /// Increment and return the next generation id.
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
    /// Return true if the provided generation id is still the latest.
    pub fn is_latest(&self, r#gen: u64) -> bool {
        self.0.load(Ordering::SeqCst) == r#gen
    }
}

// ---- Validation helpers (moved from TUI for parity) ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// E3101: required variable missing (no explicit value and no default)
    RequiredMissing { mode_id: String, var: String },
    /// E3102: enum mismatch (value not in allowed set)
    EnumMismatch {
        mode_id: String,
        var: String,
        allowed: Vec<String>,
        got: String,
    },
    /// E3106: boolean invalid (must be "true" or "false")
    BooleanInvalid {
        mode_id: String,
        var: String,
        got: String,
    },
    /// E3107: number invalid (neither integer nor float)
    NumberInvalid {
        mode_id: String,
        var: String,
        got: String,
    },
    /// E3108: path invalid (control chars or empty after trim)
    PathInvalid {
        mode_id: String,
        var: String,
        got: String,
    },
}

/// Validate a single variable's explicit value against its definition.
pub fn validate_var_value(
    mode_id: &str,
    var_def: &ModeVariableDefinition,
    value: &str,
) -> Option<ValidationError> {
    if let Some(options) = &var_def.r#enum {
        if !value.is_empty() && !options.iter().any(|o| o == value) {
            return Some(ValidationError::EnumMismatch {
                mode_id: mode_id.to_string(),
                var: var_def.name.clone(),
                allowed: options.clone(),
                got: value.to_string(),
            });
        }
    }
    match var_def.var_type {
        Some(VarType::Boolean) => {
            if !value.is_empty() {
                let s = value.to_lowercase();
                if s != "true" && s != "false" {
                    return Some(ValidationError::BooleanInvalid {
                        mode_id: mode_id.to_string(),
                        var: var_def.name.clone(),
                        got: value.to_string(),
                    });
                }
            }
        }
        Some(VarType::Number) => {
            if !value.is_empty() {
                let s = value.trim();
                if s.parse::<i64>().is_err() && s.parse::<f64>().is_err() {
                    return Some(ValidationError::NumberInvalid {
                        mode_id: mode_id.to_string(),
                        var: var_def.name.clone(),
                        got: value.to_string(),
                    });
                }
            }
        }
        Some(VarType::Path) => {
            if !value.is_empty() {
                let s = value;
                let bad = s.chars().any(char::is_control);
                if bad || s.trim().is_empty() {
                    return Some(ValidationError::PathInvalid {
                        mode_id: mode_id.to_string(),
                        var: var_def.name.clone(),
                        got: value.to_string(),
                    });
                }
            }
        }
        _ => {}
    }
    None
}

/// Validate required variables for all enabled modes using explicit values or defaults.
pub fn validate_enabled<'a>(
    defs: &'a [ModeDefinition],
    enabled: &[EnabledMode<'a>],
) -> Vec<ValidationError> {
    let mut errs: Vec<ValidationError> = Vec::new();
    for em in enabled {
        let Some(def) = defs.iter().find(|d| d.id == em.id) else {
            continue;
        };
        for v in &def.variables {
            let explicit = em.variables.get(v.name.as_str()).and_then(|o| o.clone());
            if v.required && explicit.is_none() && v.default.is_none() {
                errs.push(ValidationError::RequiredMissing {
                    mode_id: def.id.clone(),
                    var: v.name.clone(),
                });
            }
            if let Some(val) = explicit.as_deref() {
                if let Some(e) = validate_var_value(&def.id, v, val) {
                    errs.push(e);
                }
            }
        }
    }
    errs
}

/// Format a single validation error to the user-facing short code string.
/// Returns None for errors that are typically aggregated elsewhere (e.g., RequiredMissing).
pub fn format_validation_error(err: &ValidationError) -> Option<String> {
    match err {
        ValidationError::EnumMismatch {
            var, allowed, got, ..
        } => Some(format!(
            "E3102 EnumMismatch: {}={} (allowed: {})",
            var,
            got,
            allowed.join("|")
        )),
        ValidationError::BooleanInvalid { var, got, .. } => {
            Some(format!("E3106 BooleanInvalid: {}={}", var, got))
        }
        ValidationError::NumberInvalid { var, got, .. } => {
            Some(format!("E3107 NumberInvalid: {}={}", var, got))
        }
        ValidationError::PathInvalid { var, got, .. } => {
            Some(format!("E3108 PathInvalid: {}={}", var, got))
        }
        ValidationError::RequiredMissing { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn id_from_rel_path_allows_safe_chars() {
        assert_eq!(
            id_from_rel_path(Path::new("a/b/c.md")).as_deref(),
            Some("/a:b:c")
        );
        assert!(id_from_rel_path(Path::new("a/b/c x.md")).is_none());
    }

    #[test]
    fn render_injects_mode_block() {
        let def = ModeDefinition {
            id: "/demo".into(),
            display_name: Some("Demo".into()),
            description: None,
            argument_hint: None,
            kind: ModeKind::Persistent,
            default_enabled: true,
            variables: vec![ModeVariableDefinition {
                name: "who".into(),
                var_type: None,
                required: false,
                default: Some(serde_yaml::Value::String("world".into())),
                r#enum: None,
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: ModeScope::Project("app".into()),
            path: PathBuf::new(),
            body: "Hello {{who}}".into(),
        };
        let scope = def.scope.clone();
        let enabled = EnabledMode {
            id: "/demo",
            display_name: None,
            scope: &scope,
            variables: IndexMap::from([("who", None)]),
        };
        let out = render_user_instructions("base", &[enabled], &[def]).unwrap();
        assert!(out.contains("<mode_instructions>"));
        assert!(out.contains("Hello world"));
    }

    #[test]
    fn normalize_equiv_collapses_crlf_trailing_ws_and_blanks() {
        let a = "line1\r\nline2  \r\n\r\nline3\r\n\r\n\r\n";
        let b = "line1\nline2\n\nline3\n";
        assert_eq!(normalize_equiv(a), "line1\nline2\n\nline3");
        assert_eq!(normalize_equiv(b), "line1\nline2\n\nline3");
        assert!(is_equivalent(a, b));
    }

    #[test]
    fn format_modes_error_codes() {
        assert_eq!(
            format_modes_error(&ModesError::IllegalId("bad".into())),
            "E1001 IllegalId: bad"
        );
        assert_eq!(
            format_modes_error(&ModesError::Io("nope".into())),
            "E1004 Io: nope"
        );
        assert_eq!(
            format_modes_error(&ModesError::Frontmatter("yaml".into())),
            "E2001 Frontmatter: yaml"
        );
        assert_eq!(
            format_modes_error(&ModesError::VarDup("x".into())),
            "E2101 VarDup: x"
        );
        assert_eq!(
            format_modes_error(&ModesError::Regex("re".into())),
            "E2201 Regex: re"
        );
        assert_eq!(
            format_modes_error(&ModesError::UnknownMode("/m".into())),
            "E1201 UnknownMode: /m"
        );
    }

    #[test]
    fn labels_and_summary_and_applied_message() {
        let scope = ModeScope::Global;
        let a = EnabledMode {
            id: "/a",
            display_name: None,
            scope: &scope,
            variables: IndexMap::new(),
        };
        let b = EnabledMode {
            id: "/b",
            display_name: None,
            scope: &scope,
            variables: IndexMap::new(),
        };
        assert_eq!(enabled_labels(&[a, b]), "a · b");
        assert_eq!(applied_message(1), "Applied 1 mode(s)");
        assert_eq!(format_mode_summary(""), None);
        assert_eq!(format_mode_summary("a · b"), Some("Mode: a · b".into()));
    }

    #[test]
    fn validate_var_value_enforces_enum_bool_number_path() {
        let v_enum = ModeVariableDefinition {
            name: "e".into(),
            var_type: Some(VarType::Enum),
            required: false,
            default: None,
            r#enum: Some(vec!["x".into(), "y".into()]),
            shortcuts: None,
            pattern: None,
            inline_edit: None,
            mode_scoped: None,
        };
        assert_eq!(
            validate_var_value("/m", &v_enum, "z"),
            Some(ValidationError::EnumMismatch {
                mode_id: "/m".into(),
                var: "e".into(),
                allowed: vec!["x".into(), "y".into()],
                got: "z".into()
            })
        );
        assert_eq!(validate_var_value("/m", &v_enum, "x"), None);

        let v_bool = ModeVariableDefinition {
            var_type: Some(VarType::Boolean),
            name: "b".into(),
            required: false,
            default: None,
            r#enum: None,
            shortcuts: None,
            pattern: None,
            inline_edit: None,
            mode_scoped: None,
        };
        assert_eq!(
            validate_var_value("/m", &v_bool, "maybe"),
            Some(ValidationError::BooleanInvalid {
                mode_id: "/m".into(),
                var: "b".into(),
                got: "maybe".into()
            })
        );
        assert_eq!(validate_var_value("/m", &v_bool, "TRUE"), None);

        let v_num = ModeVariableDefinition {
            var_type: Some(VarType::Number),
            name: "n".into(),
            required: false,
            default: None,
            r#enum: None,
            shortcuts: None,
            pattern: None,
            inline_edit: None,
            mode_scoped: None,
        };
        assert_eq!(
            validate_var_value("/m", &v_num, "abc"),
            Some(ValidationError::NumberInvalid {
                mode_id: "/m".into(),
                var: "n".into(),
                got: "abc".into()
            })
        );
        assert_eq!(validate_var_value("/m", &v_num, "12"), None);
        assert_eq!(validate_var_value("/m", &v_num, "3.14"), None);

        let v_path = ModeVariableDefinition {
            var_type: Some(VarType::Path),
            name: "p".into(),
            required: false,
            default: None,
            r#enum: None,
            shortcuts: None,
            pattern: None,
            inline_edit: None,
            mode_scoped: None,
        };
        assert_eq!(
            validate_var_value("/m", &v_path, "\u{0007}"),
            Some(ValidationError::PathInvalid {
                mode_id: "/m".into(),
                var: "p".into(),
                got: "\u{0007}".into()
            })
        );
        assert_eq!(validate_var_value("/m", &v_path, " ok "), None);
    }

    #[test]
    fn validate_enabled_reports_missing_required() {
        let def = ModeDefinition {
            id: "/demo".into(),
            display_name: None,
            description: None,
            argument_hint: None,
            kind: ModeKind::Persistent,
            default_enabled: true,
            variables: vec![ModeVariableDefinition {
                name: "x".into(),
                var_type: None,
                required: true,
                default: None,
                r#enum: None,
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: ModeScope::Global,
            path: PathBuf::new(),
            body: String::new(),
        };
        let scope = def.scope.clone();
        let em = EnabledMode {
            id: "/demo",
            display_name: None,
            scope: &scope,
            variables: IndexMap::new(),
        };
        let errs = validate_enabled(&[def], &[em]);
        assert_eq!(
            errs,
            vec![ValidationError::RequiredMissing {
                mode_id: "/demo".into(),
                var: "x".into()
            }]
        );
    }

    #[test]
    fn format_validation_error_codes_and_none_for_required() {
        let e1 = ValidationError::EnumMismatch {
            mode_id: "/m".into(),
            var: "e".into(),
            allowed: vec!["x".into(), "y".into()],
            got: "z".into(),
        };
        assert_eq!(
            format_validation_error(&e1).as_deref(),
            Some("E3102 EnumMismatch: e=z (allowed: x|y)")
        );

        let e2 = ValidationError::BooleanInvalid {
            mode_id: "/m".into(),
            var: "b".into(),
            got: "maybe".into(),
        };
        assert_eq!(
            format_validation_error(&e2).as_deref(),
            Some("E3106 BooleanInvalid: b=maybe")
        );

        let e3 = ValidationError::NumberInvalid {
            mode_id: "/m".into(),
            var: "n".into(),
            got: "abc".into(),
        };
        assert_eq!(
            format_validation_error(&e3).as_deref(),
            Some("E3107 NumberInvalid: n=abc")
        );

        let e4 = ValidationError::PathInvalid {
            mode_id: "/m".into(),
            var: "p".into(),
            got: "\u{7}".into(),
        };
        assert_eq!(
            format_validation_error(&e4).as_deref(),
            Some("E3108 PathInvalid: p=\u{7}")
        );

        let e5 = ValidationError::RequiredMissing {
            mode_id: "/m".into(),
            var: "x".into(),
        };
        assert_eq!(format_validation_error(&e5), None);
    }

    #[test]
    fn debounce_gen_monotonic_and_latest() {
        let g = DebounceGen::new();
        let a = g.next();
        assert!(g.is_latest(a));
        let b = g.next();
        assert!(b > a);
        assert!(g.is_latest(b));
        assert!(!g.is_latest(a));
    }
}
