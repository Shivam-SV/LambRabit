//! Commands a project can run from its Sites panel: discovered from the project (package.json
//! and composer.json scripts, and the framework CLIs' own command lists), listed by the
//! template it came from, and added by the user. apps.rs runs them.

use std::{ffi::OsString, path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    apps,
    config::PackageManager,
    procgroup,
    projects::{shell, strip_ansi},
};

/// Where commands come from. Templates name them in `commands: discover:`; projects without
/// one get every source their files show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    #[serde(rename = "package.json")]
    PackageJson,
    #[serde(rename = "composer.json")]
    ComposerJson,
    /// Laravel's `php artisan`.
    #[serde(rename = "artisan")]
    Artisan,
    /// Symfony's `php bin/console`.
    #[serde(rename = "console")]
    Console,
    /// CakePHP's `bin/cake`.
    #[serde(rename = "cake")]
    Cake,
    /// CodeIgniter's `php spark`.
    #[serde(rename = "spark")]
    Spark,
    /// `php bin/magento`.
    #[serde(rename = "magento")]
    Magento,
    /// Statamic's `php please`.
    #[serde(rename = "please")]
    Please,
    /// Drupal's `vendor/bin/drush`.
    #[serde(rename = "drush")]
    Drush,
}

impl Source {
    pub const ALL: [Source; 9] = [
        Source::PackageJson,
        Source::ComposerJson,
        Source::Artisan,
        Source::Console,
        Source::Cake,
        Source::Spark,
        Source::Magento,
        Source::Please,
        Source::Drush,
    ];

    /// The file that shows a project has this source.
    fn marker(self) -> &'static str {
        match self {
            Source::PackageJson => "package.json",
            Source::ComposerJson => "composer.json",
            Source::Artisan => "artisan",
            Source::Console => "bin/console",
            Source::Cake => "bin/cake.php",
            Source::Spark => "spark",
            Source::Magento => "bin/magento",
            Source::Please => "please",
            Source::Drush => "vendor/bin/drush",
        }
    }

    fn group(self) -> &'static str {
        match self {
            Source::PackageJson => "package.json scripts",
            Source::ComposerJson => "Composer scripts",
            Source::Artisan => "Artisan",
            Source::Console => "Symfony console",
            Source::Cake => "CakePHP",
            Source::Spark => "Spark",
            Source::Magento => "Magento",
            Source::Please => "Statamic",
            Source::Drush => "Drush",
        }
    }

    /// How to run one of the CLI's commands, and how to list them (Symfony-console CLIs
    /// all answer `list --format=json`).
    fn cli(self) -> Option<(&'static str, &'static str)> {
        match self {
            Source::Artisan => Some(("php artisan", "php artisan list --format=json")),
            Source::Console => Some(("php bin/console", "php bin/console list --format=json")),
            Source::Magento => Some(("php bin/magento", "php bin/magento list --format=json")),
            Source::Please => Some(("php please", "php please list --format=json")),
            Source::Drush => Some(("php vendor/bin/drush", "php vendor/bin/drush list --format=json")),
            Source::Cake => Some(("php bin/cake.php", "php bin/cake.php")),
            Source::Spark => Some(("php spark", "php spark list --simple")),
            Source::PackageJson | Source::ComposerJson => None,
        }
    }
}

/// The sources a project's files show.
pub fn detect(dir: &Path) -> Vec<Source> {
    Source::ALL.into_iter().filter(|s| dir.join(s.marker()).exists()).collect()
}

/// A command the user added to a project (saved in its settings).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserCommand {
    pub name: String,
    pub run: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    /// Heading it's listed under ("Artisan", "package.json scripts", "Yours"…).
    pub group: String,
    pub name: String,
    /// What runs, in the project folder.
    pub command: String,
    pub description: Option<String>,
    /// "yours", "template" or "discovered".
    pub origin: &'static str,
}

/// Runs a listing command with a time limit; its stdout, or why it failed.
async fn output(dir: &Path, path: &OsString, command: &str) -> Result<String, String> {
    let (program, args) = shell(command);
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    procgroup::prepare(&mut cmd);
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    match tokio::time::timeout(Duration::from_secs(45), child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it failed");
            Err(strip_ansi(last.trim()))
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("it took longer than 45 seconds".into()),
    }
}

/// `list --format=json` from a Symfony-console CLI: its visible commands.
fn parse_console_json(text: &str) -> Vec<(String, Option<String>)> {
    // Some CLIs print a warning line before the JSON.
    let json = text.find('{').map_or(text, |i| &text[i..]);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    value["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["hidden"] != true)
        .filter_map(|c| {
            let name = c["name"].as_str()?.to_string();
            // `list`, `help` and `_complete` are about the CLI itself.
            (!["list", "help", "completion", "_complete"].contains(&name.as_str())).then(|| (name, c["description"].as_str().filter(|d| !d.is_empty()).map(str::to_string)))
        })
        .collect()
}

/// CakePHP's command list: lines like ` - cache clear`.
fn parse_cake(text: &str) -> Vec<(String, Option<String>)> {
    text.lines().filter_map(|l| l.trim().strip_prefix("- ")).map(|n| (n.trim().to_string(), None)).filter(|(n, _)| !n.is_empty() && n != "help" && n != "version").collect()
}

/// `php spark list --simple`: one command per line.
fn parse_spark(text: &str) -> Vec<(String, Option<String>)> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.contains(' ') && *l != "help" && *l != "list").map(|n| (n.to_string(), None)).collect()
}

/// Every command a project offers from the given sources. Problems (a CLI that didn't
/// answer) are returned instead of failing the whole list.
pub async fn discover(dir: &Path, sources: &[Source], path: &OsString, pm: PackageManager) -> (Vec<Command>, Vec<String>) {
    let mut commands = Vec::new();
    let mut problems = Vec::new();
    for &source in sources {
        if !dir.join(source.marker()).exists() {
            continue;
        }
        let group = source.group().to_string();
        let discovered = |name: String, command: String, description: Option<String>| Command { group: group.clone(), name, command, description, origin: "discovered" };
        match source {
            Source::PackageJson => {
                let run = apps::pm_commands(pm, path).map(|p| p.run).unwrap_or_else(|_| format!("{} run", pm.id()));
                for (name, script) in apps::scripts(dir) {
                    commands.push(discovered(name.clone(), format!("{run} {name}"), Some(script)));
                }
            }
            Source::ComposerJson => {
                let json: serde_json::Value = std::fs::read_to_string(dir.join("composer.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
                for (name, script) in json["scripts"].as_object().into_iter().flatten() {
                    // Composer runs pre-/post- hooks on its own events.
                    if name.starts_with("pre-") || name.starts_with("post-") {
                        continue;
                    }
                    let description = json["scripts-descriptions"][name].as_str().map(str::to_string).or_else(|| match script {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Array(a) => Some(a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" && ")),
                        _ => None,
                    });
                    commands.push(discovered(name.clone(), format!("composer run-script {name}"), description));
                }
            }
            cli => {
                let Some((prefix, list)) = cli.cli() else { continue };
                match output(dir, path, list).await {
                    Ok(text) => {
                        let found = match cli {
                            Source::Cake => parse_cake(&text),
                            Source::Spark => parse_spark(&text),
                            _ => parse_console_json(&text),
                        };
                        if found.is_empty() {
                            problems.push(format!("{} listed no commands.", source.group()));
                        }
                        for (name, description) in found {
                            commands.push(discovered(name.clone(), format!("{prefix} {name}"), description));
                        }
                    }
                    Err(why) => problems.push(format!("Couldn't list {} commands: {why}", source.group())),
                }
            }
        }
    }
    (commands, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_symfony_console_lists() {
        let text = r#"Deprecated: something
{"application":{"name":"Laravel"},"commands":[
  {"name":"list","description":"List commands","hidden":false},
  {"name":"migrate","description":"Run the database migrations","hidden":false},
  {"name":"secret:thing","description":"","hidden":true},
  {"name":"about","description":"","hidden":false}]}"#;
        let found = parse_console_json(text);
        assert_eq!(found, vec![("migrate".into(), Some("Run the database migrations".into())), ("about".into(), None)]);
    }

    #[test]
    fn reads_cake_and_spark_lists() {
        let cake = "Current Paths:\n\nAvailable Commands:\n\nCakePHP:\n - cache clear\n - cache clear_all\n - help\n";
        assert_eq!(parse_cake(cake).into_iter().map(|c| c.0).collect::<Vec<_>>(), vec!["cache clear", "cache clear_all"]);
        let spark = "cache:clear\nmigrate\n\nhelp\nlist\n";
        assert_eq!(parse_spark(spark).into_iter().map(|c| c.0).collect::<Vec<_>>(), vec!["cache:clear", "migrate"]);
    }

    #[test]
    fn sources_from_files() {
        let dir = std::env::temp_dir().join(format!("lamprabit-cmd-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        for f in ["package.json", "composer.json", "artisan", "bin/console"] {
            std::fs::write(dir.join(f), "{}").unwrap();
        }
        assert_eq!(detect(&dir), vec![Source::PackageJson, Source::ComposerJson, Source::Artisan, Source::Console]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
