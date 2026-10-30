//! Project templates: YAML files describing how to create a project, step by step. The
//! built-in ones ship in the app (src-tauri/templates/); your own go in `<root>/templates/`.
//! docs/templates.md is the reference for the format; projects.rs runs the steps.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    layout::Layout,
};

/// The format version this app understands (`schema:` in a template).
pub const SCHEMA: u32 = 1;

/// Built-in templates, in the order the wizard shows them.
const BUILT_IN: &[(&str, &str)] = &[
    ("blank.yaml", include_str!("../templates/blank.yaml")),
    ("laravel.yaml", include_str!("../templates/laravel.yaml")),
    ("symfony.yaml", include_str!("../templates/symfony.yaml")),
    ("wordpress.yaml", include_str!("../templates/wordpress.yaml")),
    ("cakephp.yaml", include_str!("../templates/cakephp.yaml")),
    ("codeigniter.yaml", include_str!("../templates/codeigniter.yaml")),
    ("drupal.yaml", include_str!("../templates/drupal.yaml")),
    ("statamic.yaml", include_str!("../templates/statamic.yaml")),
    ("static.yaml", include_str!("../templates/static.yaml")),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The template's own version, recorded with each project created from it.
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// A hint shown in the wizard ("Composer · about a minute").
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub requires: Requires,
    #[serde(default)]
    pub database: DatabaseUse,
    #[serde(default)]
    pub variables: Vec<Variable>,
    pub steps: Vec<Step>,
    /// Shown when the project is ready (what to do next).
    #[serde(default)]
    pub done: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Lowest PHP version ("8.2"), or "any"; absent means the project doesn't use PHP.
    pub php: Option<String>,
    /// PHP extensions that must be loaded (checked with `php -m`).
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseUse {
    /// No database offered.
    #[default]
    None,
    /// The wizard offers one, on by default when a database server is installed.
    Optional,
    /// The project can't work without one.
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Variable {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: VariableKind,
    /// For `choice`.
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableKind {
    #[default]
    Text,
    Choice,
    Bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// Names an optional step in requests; defaults to its position.
    #[serde(default)]
    pub id: Option<String>,
    /// The progress heading ("Running database migrations").
    #[serde(default)]
    pub name: Option<String>,
    /// Runs only when this holds: `database`, `!database`, `vars.kit == react`, `vars.kit != none`.
    #[serde(default, rename = "if")]
    pub when: Option<String>,
    /// The wizard shows a checkbox for it.
    #[serde(default)]
    pub optional: bool,
    /// The checkbox text for an optional step.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether an optional step starts ticked (default yes).
    #[serde(default)]
    pub default: Option<bool>,
    /// Tools the step needs on PATH: `node` (npm, npx), `git`.
    #[serde(default)]
    pub needs: Vec<String>,
    /// Working folder: `www` or a folder inside the project; the project folder by default.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Composer with the project's PHP: `composer: [create-project, laravel/laravel, "{{project}}"]`.
    Composer(Vec<String>),
    /// The project's PHP: `php: [artisan, migrate, --force]`.
    Php(Vec<String>),
    /// A shell command (cmd on Windows, sh elsewhere), with php, composer and mysql on PATH.
    Run(String),
    Download(Download),
    Write(FileContent),
    Append(FileContent),
    Copy(CopyFile),
    /// Sets keys in a .env-style file, adding missing ones.
    Env(EnvFile),
    /// Replaces text in a file; each occurrence gets a freshly rendered value.
    Replace(ReplaceText),
    /// Creates the database chosen in the wizard (skipped when none was).
    CreateDatabase(Empty),
    /// `git init`, and a first commit when Git knows who you are.
    GitInit(GitInit),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub sha256_url: Option<String>,
    #[serde(default)]
    pub sha1_url: Option<String>,
    /// Unpack a .zip or .tar.gz into the project (else save it as `to`).
    #[serde(default)]
    pub extract: bool,
    /// Take the contents of this folder inside the archive (WordPress: `wordpress`).
    #[serde(default)]
    pub strip: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyFile {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvFile {
    #[serde(default = "dot_env")]
    pub file: String,
    pub values: serde_yaml_ng::Mapping,
}

fn dot_env() -> String {
    ".env".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceText {
    pub file: String,
    pub values: serde_yaml_ng::Mapping,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitInit {
    #[serde(default)]
    pub commit: Option<String>,
}

impl Step {
    pub fn key(&self, index: usize) -> String {
        self.id.clone().unwrap_or_else(|| index.to_string())
    }

    /// The progress heading: the template's, else one describing the action.
    pub fn heading(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        match &self.action {
            Action::Composer(args) => format!("composer {}", args.first().map(String::as_str).unwrap_or_default()),
            Action::Php(args) => format!("php {}", args.join(" ")),
            Action::Run(cmd) => cmd.clone(),
            Action::Download(d) => format!("Downloading {}", d.url.rsplit('/').next().unwrap_or(&d.url)),
            Action::Write(f) => format!("Writing {}", f.path),
            Action::Append(f) => format!("Updating {}", f.path),
            Action::Copy(c) => format!("Copying {} to {}", c.from, c.to),
            Action::Env(e) => format!("Configuring {}", e.file),
            Action::Replace(r) => format!("Updating {}", r.file),
            Action::CreateDatabase(_) => "Creating the database".into(),
            Action::GitInit(_) => "Initialising a Git repository".into(),
        }
    }

    /// The command or URL behind a step, as written in the template.
    pub fn what_runs(&self) -> String {
        match &self.action {
            Action::Run(cmd) => cmd.clone(),
            Action::Download(d) => d.url.clone(),
            _ => self.heading(),
        }
    }

    /// Every string the step renders, for checking placeholders when the template loads.
    fn strings(&self) -> Vec<String> {
        let mut out: Vec<String> = self.when.iter().chain(&self.cwd).cloned().collect();
        let map = |m: &serde_yaml_ng::Mapping| -> Vec<String> {
            m.iter().flat_map(|(k, v)| [scalar(k), scalar(v)]).flatten().collect()
        };
        match &self.action {
            Action::Composer(a) | Action::Php(a) => out.extend(a.iter().cloned()),
            Action::Run(c) => out.push(c.clone()),
            Action::Download(d) => out.extend([Some(d.url.clone()), d.to.clone(), d.strip.clone()].into_iter().flatten()),
            Action::Write(f) | Action::Append(f) => out.extend([f.path.clone(), f.content.clone()]),
            Action::Copy(c) => out.extend([c.from.clone(), c.to.clone()]),
            Action::Env(e) => {
                out.push(e.file.clone());
                out.extend(map(&e.values));
            }
            Action::Replace(r) => {
                out.push(r.file.clone());
                out.extend(map(&r.values));
            }
            Action::CreateDatabase(_) => {}
            Action::GitInit(g) => out.extend(g.commit.clone()),
        }
        out
    }
}

/// A YAML scalar as text (numbers and booleans included), for map keys and values.
pub fn scalar(v: &serde_yaml_ng::Value) -> Option<String> {
    match v {
        serde_yaml_ng::Value::String(s) => Some(s.clone()),
        serde_yaml_ng::Value::Number(n) => Some(n.to_string()),
        serde_yaml_ng::Value::Bool(b) => Some(b.to_string()),
        serde_yaml_ng::Value::Null => Some(String::new()),
        _ => None,
    }
}

/// Placeholder names every template can use (besides `vars.<id>` and `random:<length>`).
pub const PLACEHOLDERS: &[&str] = &[
    "project",
    "domain",
    "url",
    "www",
    "dir",
    "php_version",
    "database",
    "db.engine",
    "db.host",
    "db.port",
    "db.user",
    "db.password",
    "db.server_version",
    "mail.host",
    "mail.port",
];

impl Template {
    pub fn uses_php(&self) -> bool {
        self.requires.php.is_some()
    }

    /// Catches mistakes when the template loads rather than halfway through a project:
    /// the schema, ids, duplicate step ids and unknown placeholders.
    fn check(&self) -> Result<()> {
        if self.schema != SCHEMA {
            return Err(Error::msg(format!("schema {} isn't supported (this lampRabit reads schema {SCHEMA})", self.schema)));
        }
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(Error::msg("id must use letters, numbers, dashes and underscores"));
        }
        if self.steps.is_empty() {
            return Err(Error::msg("has no steps"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, step) in self.steps.iter().enumerate() {
            if !ids.insert(step.key(i)) {
                return Err(Error::msg(format!("step id \"{}\" is used twice", step.key(i))));
            }
            for tool in &step.needs {
                if !["node", "git"].contains(&tool.as_str()) {
                    return Err(Error::msg(format!("step {}: needs \"{tool}\" isn't known (use node or git)", i + 1)));
                }
            }
            for text in step.strings() {
                for name in placeholders(&text) {
                    let known = PLACEHOLDERS.contains(&name.as_str())
                        || name.strip_prefix("vars.").is_some_and(|v| self.variables.iter().any(|x| x.id == v))
                        || name.strip_prefix("random:").is_some_and(|n| n.parse::<usize>().is_ok_and(|n| (1..=256).contains(&n)));
                    if !known {
                        return Err(Error::msg(format!("step {}: unknown placeholder {{{{{name}}}}}", i + 1)));
                    }
                }
            }
            if let Some(when) = &step.when {
                parse_condition(when).map_err(|e| Error::msg(format!("step {}: {e}", i + 1)))?;
            }
        }
        for v in &self.variables {
            if v.kind == VariableKind::Choice && v.options.is_empty() {
                return Err(Error::msg(format!("variable {} is a choice without options", v.id)));
            }
        }
        Ok(())
    }

    /// Risky things a template does, each with why; asked about before running a template
    /// that didn't ship with lampRabit.
    pub fn risks(&self) -> Vec<Risk> {
        self.steps
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let reasons = step_risks(s);
                (!reasons.is_empty()).then(|| Risk { step: s.key(i), heading: s.heading(), command: s.what_runs(), reasons })
            })
            .collect()
    }
}

/// `{{ name }}` occurrences in a string.
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else { break };
        out.push(rest[start + 2..start + end].trim().to_string());
        rest = &rest[start + end + 2..];
    }
    out
}

/// Commands that can do damage outside the new project, or run code from the internet.
const RISKY: &[(&str, &str)] = &[
    ("rm -rf", "deletes files recursively"),
    ("rm -r", "deletes files recursively"),
    ("rm -fr", "deletes files recursively"),
    ("rmdir /s", "deletes folders recursively"),
    ("rd /s", "deletes folders recursively"),
    ("del /s", "deletes files recursively"),
    ("remove-item", "deletes files"),
    ("sudo ", "runs as administrator"),
    ("runas", "runs as administrator"),
    ("mkfs", "formats a disk"),
    ("format ", "can format a disk"),
    ("dd if=", "writes raw disk data"),
    ("| sh", "runs a script piped from another command"),
    ("|sh", "runs a script piped from another command"),
    ("| bash", "runs a script piped from another command"),
    ("|bash", "runs a script piped from another command"),
    ("iex", "runs downloaded PowerShell code"),
    ("invoke-expression", "runs downloaded PowerShell code"),
    ("curl ", "downloads from the internet"),
    ("wget ", "downloads from the internet"),
    ("invoke-webrequest", "downloads from the internet"),
    ("chmod -r 777", "makes files writable by everyone"),
    ("chown -r", "changes file owners recursively"),
    ("reg ", "edits the Windows registry"),
    ("setx ", "changes environment variables for your account"),
    ("shutdown", "shuts the computer down"),
    ("reboot", "restarts the computer"),
    ("/etc/", "touches system files"),
    ("c:\\windows", "touches system files"),
    ("%systemroot%", "touches system files"),
    ("..", "reaches outside the project folder"),
    ("~/", "touches your home folder"),
    ("$home", "touches your home folder"),
    ("%userprofile%", "touches your home folder"),
    ("eval ", "runs generated code"),
];

/// Why a step is risky; one line per reason ("rm -rf" and "rm -r" are one reason).
fn step_risks(step: &Step) -> Vec<String> {
    match &step.action {
        Action::Run(cmd) => {
            let lower = cmd.to_lowercase();
            let mut reasons: Vec<String> = Vec::new();
            for (pattern, reason) in RISKY {
                if lower.contains(pattern) && !reasons.iter().any(|r| r == reason) {
                    reasons.push(reason.to_string());
                }
            }
            reasons
        }
        Action::Download(d) => {
            let mut reasons = Vec::new();
            if !d.url.starts_with("https://") {
                reasons.push("downloads over an unencrypted connection".into());
            }
            if d.sha256.is_none() && d.sha256_url.is_none() && d.sha1_url.is_none() {
                reasons.push("downloads a file without checking its checksum".into());
            }
            reasons
        }
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    pub step: String,
    pub heading: String,
    /// What actually runs or is downloaded, shown for approval (a step's name could say anything).
    pub command: String,
    pub reasons: Vec<String>,
}

/// A parsed `if:` condition.
#[derive(Debug, PartialEq)]
enum Condition {
    Truthy(String, bool),
    Compare(String, String, bool),
}

fn parse_condition(text: &str) -> Result<Condition> {
    let text = text.trim();
    let name = |s: &str| -> Result<String> {
        let s = s.trim().trim_start_matches("{{").trim_end_matches("}}").trim();
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "._".contains(c)) {
            return Err(Error::msg(format!("can't read the condition \"{text}\"")));
        }
        Ok(s.to_string())
    };
    for (op, equal) in [("!=", false), ("==", true)] {
        if let Some((lhs, rhs)) = text.split_once(op) {
            return Ok(Condition::Compare(name(lhs)?, rhs.trim().trim_matches(['"', '\'']).to_string(), equal));
        }
    }
    Ok(match text.strip_prefix('!') {
        Some(rest) => Condition::Truthy(name(rest)?, false),
        None => Condition::Truthy(name(text)?, true),
    })
}

/// Values for placeholders and conditions.
pub struct Values(pub BTreeMap<String, String>);

impl Values {
    pub fn get(&self, name: &str) -> String {
        self.0.get(name).cloned().unwrap_or_default()
    }

    pub fn holds(&self, condition: &str) -> Result<bool> {
        let truthy = |v: &str| !v.is_empty() && v != "false" && v != "none";
        Ok(match parse_condition(condition)? {
            Condition::Truthy(name, want) => truthy(&self.get(&name)) == want,
            Condition::Compare(name, value, equal) => (self.get(&name) == value) == equal,
        })
    }

    /// Fills in `{{ name }}` placeholders; `{{random:N}}` is a fresh secret each time.
    pub fn render(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            let Some(end) = rest[start..].find("}}") else { break };
            out.push_str(&rest[..start]);
            let name = rest[start + 2..start + end].trim();
            match name.strip_prefix("random:").and_then(|n| n.parse().ok()) {
                Some(len) => out.push_str(&crate::projects::random_string(len)),
                None => out.push_str(&self.get(name)),
            }
            rest = &rest[start + end + 2..];
        }
        out.push_str(rest);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    BuiltIn,
    User,
}

/// A template as the wizard sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: u32,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub duration: Option<String>,
    pub source: Source,
    /// Where a user template's file is.
    pub path: Option<PathBuf>,
    pub uses_php: bool,
    pub min_php: Option<String>,
    pub extensions: Vec<String>,
    pub database: DatabaseUse,
    pub variables: Vec<Variable>,
    pub optional_steps: Vec<OptionalStep>,
    /// Steps that need approval before running (user templates only).
    pub risks: Vec<Risk>,
    pub done: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalStep {
    pub id: String,
    pub label: String,
    pub default: bool,
}

/// A user template that couldn't be read, with why.
#[derive(Debug, Clone, Serialize)]
pub struct Broken {
    pub file: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub templates: Vec<TemplateInfo>,
    pub broken: Vec<Broken>,
    /// The folder for your own templates.
    pub folder: PathBuf,
}

pub fn folder(layout: &Layout) -> PathBuf {
    layout.root().join("templates")
}

fn parse(text: &str) -> Result<Template> {
    let template: Template = serde_yaml_ng::from_str(text).map_err(|e| Error::msg(e.to_string()))?;
    template.check()?;
    Ok(template)
}

/// Built-in templates, then the user's (a user template with a built-in's id replaces it).
pub fn load(layout: &Layout) -> (Vec<(Template, Source, Option<PathBuf>)>, Vec<Broken>) {
    let mut all: Vec<(Template, Source, Option<PathBuf>)> = BUILT_IN
        .iter()
        .map(|(file, text)| (parse(text).unwrap_or_else(|e| panic!("built-in template {file}: {e}")), Source::BuiltIn, None))
        .collect();
    let mut broken = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder(layout))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml" || e == "yml"))
        .collect();
    files.sort();
    for file in files {
        match std::fs::read_to_string(&file).map_err(Error::from).and_then(|t| parse(&t)) {
            Ok(t) => {
                all.retain(|(other, _, _)| other.id != t.id);
                all.push((t, Source::User, Some(file)));
            }
            Err(e) => broken.push(Broken { file, error: e.to_string() }),
        }
    }
    (all, broken)
}

pub fn catalog(layout: &Layout) -> Catalog {
    let (all, broken) = load(layout);
    let templates = all
        .into_iter()
        .map(|(t, source, path)| TemplateInfo {
            risks: if source == Source::User { t.risks() } else { Vec::new() },
            optional_steps: t
                .steps
                .iter()
                .enumerate()
                .filter(|(_, s)| s.optional)
                .map(|(i, s)| OptionalStep { id: s.key(i), label: s.label.clone().unwrap_or_else(|| s.heading()), default: s.default.unwrap_or(true) })
                .collect(),
            id: t.id.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            version: t.version,
            icon: t.icon.clone(),
            color: t.color.clone(),
            duration: t.duration.clone(),
            source,
            path,
            uses_php: t.uses_php(),
            min_php: t.requires.php.clone().filter(|v| v != "any"),
            extensions: t.requires.extensions.clone(),
            database: t.database,
            variables: t.variables.clone(),
            done: t.done.clone(),
        })
        .collect();
    Catalog { templates, broken, folder: folder(layout) }
}

/// The template with this id, and whether it shipped with lampRabit.
pub fn find(layout: &Layout, id: &str) -> Result<(Template, Source)> {
    load(layout)
        .0
        .into_iter()
        .find(|(t, _, _)| t.id == id)
        .map(|(t, source, _)| (t, source))
        .ok_or_else(|| Error::msg(format!("There's no template \"{id}\". Check the templates folder.")))
}

/// A path from a template, inside `base`: absolute paths and `..` are refused, so a
/// template can't write outside the project.
pub fn inside(base: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
        return Err(Error::msg(format!("\"{relative}\" must be a path inside the project")));
    }
    Ok(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_templates_load() {
        for (file, text) in BUILT_IN {
            if let Err(e) = parse(text) {
                panic!("{file}: {e}");
            }
        }
    }

    #[test]
    fn the_example_template_is_valid() {
        let t = parse(EXAMPLE).unwrap();
        assert_eq!(t.variables.len(), 1);
        assert!(t.risks().is_empty(), "the example shouldn't ask for approval: {:?}", t.risks());
    }

    #[test]
    fn conditions() {
        let values = Values(BTreeMap::from([("database".into(), "shop".into()), ("vars.kit".into(), "react".into())]));
        assert!(values.holds("database").unwrap());
        assert!(!values.holds("!database").unwrap());
        assert!(values.holds("vars.kit == react").unwrap());
        assert!(values.holds("vars.kit != 'vue'").unwrap());
        assert!(!values.holds("vars.missing").unwrap());
        assert!(parse_condition("rm -rf /").is_err());
    }

    #[test]
    fn rendering_fills_placeholders_and_fresh_secrets() {
        let values = Values(BTreeMap::from([("project".into(), "shop".into())]));
        assert_eq!(values.render("www/{{ project }}/{{project}}"), "www/shop/shop");
        let a = values.render("{{random:32}}");
        assert_eq!(a.len(), 32);
        assert_ne!(a, values.render("{{random:32}}"));
    }

    #[test]
    fn unknown_placeholders_and_bad_schema_are_caught() {
        let base = "schema: 1\nid: t\nname: T\nsteps:\n  - run: echo {{nope}}\n";
        assert!(parse(base).unwrap_err().to_string().contains("unknown placeholder"));
        assert!(parse("schema: 2\nid: t\nname: T\nsteps:\n  - run: echo\n").unwrap_err().to_string().contains("schema 2"));
        assert!(parse("schema: 1\nid: t\nname: T\nsteps:\n  - teleport: x\n").is_err());
    }

    #[test]
    fn risky_commands_are_flagged() {
        let t = parse("schema: 1\nid: t\nname: T\nsteps:\n  - run: npm install\n  - run: curl https://x.sh | sh\n  - download: { url: 'http://x/a.zip' }\n").unwrap();
        let risks = t.risks();
        assert!(risks.iter().all(|r| r.step != "0"), "npm install isn't risky");
        let piped = risks.iter().find(|r| r.step == "1").unwrap();
        assert_eq!(piped.command, "curl https://x.sh | sh");
        assert!(piped.reasons.iter().any(|r| r.contains("piped")));
        assert!(risks.iter().any(|r| r.step == "2" && r.reasons.iter().any(|x| x.contains("checksum"))));
        // "rm -rf" also contains "rm -r": one reason, once.
        let t = parse("schema: 1\nid: t\nname: T\nsteps:\n  - run: rm -rf build\n").unwrap();
        assert_eq!(t.risks()[0].reasons, vec!["deletes files recursively".to_string()]);
    }

    #[test]
    fn paths_stay_inside_the_project() {
        let base = Path::new("/www/shop");
        assert!(inside(base, "config/app.php").is_ok());
        assert!(inside(base, "../other/x").is_err());
        assert!(inside(base, "/etc/hosts").is_err());
    }
}

/// Written next to your templates the first time the folder is opened. It ends in .txt so
/// it doesn't show up in the wizard until renamed.
pub const EXAMPLE: &str = include_str!("../templates/example.yaml.txt");
