//! Deterministic compiler for the two Brain-owned sections of `AGENTS.md`
//! (`## Personal Rules (Brain)` and `## Playbooks (Read Before Task)`) plus the
//! trailing `brain-sync` marker. Everything else in `AGENTS.md` (Commands,
//! Architecture, Repo Guardrails, Done Criteria) is repo knowledge written by
//! the AI and is preserved byte for byte.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::brain_core::detect_stubs;

pub const PERSONAL_HEADING: &str = "## Personal Rules (Brain)";
pub const PLAYBOOKS_HEADING: &str = "## Playbooks (Read Before Task)";
const GUARDRAILS_PREFIX: &str = "## Repo Guardrails";
const DONE_PREFIX: &str = "## Done Criteria";

pub const KNOWN_TRAITS: &[&str] = &[
    "always",
    "desktop-gui",
    "windows-app",
    "web-ui",
    "browser-extension",
    "android-app",
    "cli-backend",
    "has-github-remote",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleMode {
    Inline,
    Reference,
}

#[derive(Debug, Clone)]
pub struct RuleFile {
    pub name: String,
    pub apply_when: Vec<String>,
    pub mode: RuleMode,
    pub trigger: Option<String>,
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CompileReport {
    pub traits: Vec<String>,
    pub inline_rules: usize,
    pub playbooks: usize,
    pub overridden_topics: Vec<String>,
    pub warnings: Vec<String>,
}

fn has_dep(pkg: &serde_json::Value, names: &[&str]) -> bool {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .filter_map(|k| pkg.get(*k).and_then(|v| v.as_object()))
        .any(|deps| deps.keys().any(|k| names.iter().any(|n| k == n || k.starts_with(&format!("{n}/")))))
}

fn read_lossy(path: &Path) -> String {
    fs::read(path)
        .map(|b| crate::brain_core::decode_text(&b))
        .unwrap_or_default()
}

/// Traits are plain filesystem facts so the same repo always compiles the same way.
pub fn detect_traits(repo: &Path) -> BTreeSet<String> {
    let mut t = BTreeSet::new();
    t.insert("always".to_string());

    let pkg: serde_json::Value = serde_json::from_str(
        read_lossy(&repo.join("package.json")).trim_start_matches('\u{FEFF}'),
    )
    .unwrap_or(serde_json::Value::Null);
    let pyproject = read_lossy(&repo.join("pyproject.toml")).to_ascii_lowercase();
    let requirements = read_lossy(&repo.join("requirements.txt")).to_ascii_lowercase();
    let cargo = format!(
        "{}\n{}",
        read_lossy(&repo.join("Cargo.toml")),
        read_lossy(&repo.join("src-tauri").join("Cargo.toml"))
    )
    .to_ascii_lowercase();

    let has_tauri = repo.join("src-tauri").is_dir();
    let has_electron = has_dep(&pkg, &["electron"]);
    let py_gui = ["pyside6", "pyside2", "pyqt5", "pyqt6", "tkinter", "customtkinter"]
        .iter()
        .any(|n| pyproject.contains(n) || requirements.contains(n));
    let has_csproj = dir_has_ext(repo, "csproj");
    let dotnet_gui = has_csproj && {
        let mut found = false;
        if let Ok(entries) = fs::read_dir(repo) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("csproj")) {
                    let c = read_lossy(&p);
                    if c.contains("UseWPF") || c.contains("UseWindowsForms") || c.contains("WindowsAppSDK") {
                        found = true;
                    }
                }
            }
        }
        found
    };
    let native_gui = ["eframe", "egui", "iced", "slint", "winit"]
        .iter()
        .any(|n| cargo.contains(&format!("\n{n} ")) || cargo.contains(&format!("\n{n}=")));
    if has_tauri || has_electron || py_gui || dotnet_gui || native_gui {
        t.insert("desktop-gui".to_string());
    }

    if has_tauri
        || repo.join("gloria.toml").is_file()
        || dir_has_ext(repo, "iss")
        || has_csproj
        || cargo.contains("\nwindows ")
        || cargo.contains("\nwindows-sys")
        || cargo.contains("\nwinapi")
    {
        t.insert("windows-app".to_string());
    }

    let manifest = read_lossy(&repo.join("manifest.json"));
    if manifest.contains("\"manifest_version\"") {
        t.insert("browser-extension".to_string());
    }

    if has_dep(&pkg, &["react", "vue", "svelte", "next", "vite", "preact", "solid-js", "@angular/core"])
        || repo.join("index.html").is_file()
    {
        t.insert("web-ui".to_string());
    }

    if repo.join("build.gradle").is_file()
        || repo.join("build.gradle.kts").is_file()
        || repo.join("app").join("src").join("main").join("AndroidManifest.xml").is_file()
        || repo.join("AndroidManifest.xml").is_file()
    {
        t.insert("android-app".to_string());
    }

    let gui = ["desktop-gui", "web-ui", "browser-extension", "android-app"];
    if !gui.iter().any(|g| t.contains(*g)) {
        t.insert("cli-backend".to_string());
    }

    let git_config = read_lossy(&repo.join(".git").join("config")).to_ascii_lowercase();
    if repo.join(".github").is_dir() || git_config.contains("github.com") {
        t.insert("has-github-remote".to_string());
    }
    t
}

fn dir_has_ext(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case(ext))
            })
        })
        .unwrap_or(false)
}

fn split_list(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn parse_rule(name: &str, raw: &str) -> RuleFile {
    let text = raw
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut apply_when = vec!["always".to_string()];
    let mut mode: Option<RuleMode> = None;
    let mut trigger = None;
    let mut body = text.as_str();

    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n").map(|i| (i, i + 5)).or_else(|| {
            rest.strip_suffix("\n---").map(|s| (s.len(), rest.len()))
        }) {
            let front = &rest[..end.0];
            body = &rest[end.1..];
            for line in front.lines() {
                let Some((k, v)) = line.split_once(':') else { continue };
                match k.trim().to_ascii_lowercase().as_str() {
                    "apply_when" => apply_when = split_list(v),
                    "mode" => {
                        mode = Some(if v.trim().eq_ignore_ascii_case("reference") {
                            RuleMode::Reference
                        } else {
                            RuleMode::Inline
                        })
                    }
                    "trigger" => {
                        let t = v.trim().trim_matches('"').trim();
                        if !t.is_empty() {
                            trigger = Some(t.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let body = body.trim_matches('\n').to_string();
    let mode = mode.unwrap_or(if body.lines().count() <= 15 {
        RuleMode::Inline
    } else {
        RuleMode::Reference
    });
    let title = body
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()));

    RuleFile {
        name: name.to_string(),
        apply_when,
        mode,
        trigger,
        title,
        body,
    }
}

/// Top-level `- ` bullets with their indented continuation lines.
fn bullets(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        if line.starts_with("- ") {
            out.push(line.trim_end().to_string());
        } else if let Some(last) = out.last_mut() {
            if line.starts_with(' ') || line.starts_with('\t') {
                if !line.trim().is_empty() {
                    last.push('\n');
                    last.push_str(line.trim_end());
                }
            } else if !line.trim().is_empty() {
                // A heading or paragraph ends the bullet list item.
                out.push(String::new());
            }
        }
    }
    out.retain(|b| !b.is_empty());
    out
}

fn tag_re() -> Regex {
    Regex::new(r"^- \[([A-Za-z0-9_, -]+)\][ \t]+").expect("valid tag regex")
}

pub fn topic_of(bullet: &str) -> Option<String> {
    let re = Regex::new(r"^- (?:\[[^\]]+\][ \t]+)?\*\*([^*\n]+?):?\*\*").expect("valid topic regex");
    re.captures(bullet)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().trim_end_matches(':').trim().to_ascii_lowercase())
}

fn matches(conds: &[String], traits: &BTreeSet<String>) -> bool {
    conds.iter().any(|c| c == "always" || traits.contains(c))
}

fn section_bounds(lines: &[&str], pred: impl Fn(&str) -> bool) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| pred(l.trim_end()))?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with("## ") || is_marker_line(l))
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    Some((start, end))
}

pub fn is_marker_line(line: &str) -> bool {
    Regex::new(r"^\s*<!--\s*brain-sync:")
        .expect("valid marker regex")
        .is_match(line)
}

fn guardrail_topics(agents: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = agents.lines().collect();
    let Some((s, e)) = section_bounds(&lines, |l| l.starts_with(GUARDRAILS_PREFIX)) else {
        return BTreeSet::new();
    };
    lines[s + 1..e].iter().filter_map(|l| topic_of(l)).collect()
}

fn pointer_title(rule: &RuleFile) -> String {
    let raw = rule.title.clone().unwrap_or_else(|| {
        rule.name
            .trim_end_matches(".md")
            .split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    // "Building & Packaging (GloriaSetup)" -> "Building & Packaging"
    let re = Regex::new(r"\s*\([^)]*\)\s*$").expect("valid paren regex");
    re.replace(&raw, "").trim().to_string()
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Builds the inline bullets and playbook pointers for `traits`, dropping any brain
/// bullet whose `**Topic:**` the repo's own guardrails already define.
pub fn compile_rules(
    rules: &[RuleFile],
    traits: &BTreeSet<String>,
    override_topics: &BTreeSet<String>,
    report: &mut CompileReport,
) -> (Vec<String>, Vec<String>) {
    let mut sorted: Vec<&RuleFile> = rules.iter().collect();
    sorted.sort_by_key(|a| a.name.to_ascii_lowercase());
    let tag = tag_re();
    let mut inline = Vec::new();
    let mut pointers = Vec::new();

    for rule in sorted {
        for c in &rule.apply_when {
            if !KNOWN_TRAITS.contains(&c.as_str()) {
                report
                    .warnings
                    .push(format!("{}: unknown apply_when trait '{c}'", rule.name));
            }
        }
        let file_match = matches(&rule.apply_when, traits);
        match rule.mode {
            RuleMode::Inline => {
                for b in bullets(&rule.body) {
                    let first = b.lines().next().unwrap_or_default();
                    if !detect_stubs(first).is_empty() {
                        continue;
                    }
                    let (keep, text) = match tag.captures(&b) {
                        Some(c) => {
                            let conds = split_list(&c[1]);
                            let rest = format!("- {}", &b[c.get(0).unwrap().end()..]);
                            (matches(&conds, traits), rest)
                        }
                        None => (file_match, b.clone()),
                    };
                    if !keep {
                        continue;
                    }
                    if let Some(topic) = topic_of(&text) {
                        if override_topics.contains(&topic) {
                            report.overridden_topics.push(topic);
                            continue;
                        }
                    }
                    inline.push(text);
                }
            }
            RuleMode::Reference => {
                if !file_match {
                    continue;
                }
                let all = bullets(&rule.body);
                let filled = all
                    .iter()
                    .filter(|b| detect_stubs(b.lines().next().unwrap_or_default()).is_empty())
                    .count();
                if filled == 0 && (!all.is_empty() || rule.body.trim().is_empty()) {
                    continue;
                }
                let title = pointer_title(rule);
                if override_topics.contains(&title.to_ascii_lowercase()) {
                    report.overridden_topics.push(title.to_ascii_lowercase());
                    continue;
                }
                let trigger = rule
                    .trigger
                    .as_deref()
                    .map(|t| lower_first(t.trim().trim_end_matches('.')))
                    .unwrap_or_else(|| "before working in this area".to_string());
                pointers.push(format!(
                    "- **{title}:** Read `.brain/{}` {trigger}.",
                    rule.name
                ));
            }
        }
    }
    report.inline_rules = inline.len();
    report.playbooks = pointers.len();
    (inline, pointers)
}

fn replace_section(lines: &mut Vec<String>, heading_prefix: &str, default_heading: &str, items: &[String]) {
    let view: Vec<&str> = lines.iter().map(String::as_str).collect();
    let bounds = section_bounds(&view, |l| l.starts_with(heading_prefix));
    let mut block: Vec<String> = Vec::new();
    if !items.is_empty() {
        let heading = bounds
            .map(|(s, _)| lines[s].trim_end().to_string())
            .unwrap_or_else(|| default_heading.to_string());
        block.push(heading);
        for it in items {
            block.extend(it.lines().map(str::to_string));
        }
        block.push(String::new());
    }

    match bounds {
        Some((s, e)) => {
            lines.splice(s..e, block);
        }
        None if !block.is_empty() => {
            let view: Vec<&str> = lines.iter().map(String::as_str).collect();
            let at = view
                .iter()
                .position(|l| l.starts_with(GUARDRAILS_PREFIX))
                .or_else(|| view.iter().position(|l| l.starts_with(DONE_PREFIX)))
                .or_else(|| view.iter().position(|l| is_marker_line(l)))
                .unwrap_or(view.len());
            if at > 0 && !lines[at - 1].trim().is_empty() {
                block.insert(0, String::new());
            }
            lines.splice(at..at, block);
        }
        None => {}
    }
}

/// Rewrites the Brain-owned sections and the marker of an `AGENTS.md` text.
/// Returns the new text (LF) — the caller decides on line endings.
pub fn compile_agents_text(
    agents: &str,
    rules: &[RuleFile],
    traits: &BTreeSet<String>,
    marker: &str,
    report: &mut CompileReport,
) -> String {
    let norm = agents
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let overrides = guardrail_topics(&norm);
    let (inline, pointers) = compile_rules(rules, traits, &overrides, report);
    report.traits = traits.iter().cloned().collect();

    let mut lines: Vec<String> = norm
        .lines()
        .filter(|l| !is_marker_line(l))
        .map(str::to_string)
        .collect();
    replace_section(&mut lines, "## Personal Rules", PERSONAL_HEADING, &inline);
    replace_section(&mut lines, "## Playbooks", PLAYBOOKS_HEADING, &pointers);

    // Collapse runs of blank lines and trim the end, then append the marker.
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    for l in lines {
        if l.trim().is_empty() && out.last().is_some_and(|p: &String| p.trim().is_empty()) {
            continue;
        }
        out.push(l);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.push(String::new());
    out.push(marker.to_string());
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn tags_traits_stubs_and_overrides() {
        let ui = parse_rule(
            "ui.md",
            "---\napply_when: desktop-gui, web-ui\nmode: inline\n---\n- [desktop-gui] **Chrome:** frameless\n- **Scrollbars:** thin\n- **Empty:** <fill>\n",
        );
        let wf = parse_rule("workflow.md", "---\napply_when: always\nmode: inline\n---\n- **Git Branching:** main\n- **Versioning:** later\n  continued\n");
        let refr = parse_rule(
            "build.md",
            "---\napply_when: windows-app\nmode: reference\ntrigger: Before packaging\n---\n# Building & Packaging (GloriaSetup)\n- **A:** b\n",
        );
        let stub_ref = parse_rule("stub.md", "---\nmode: reference\n---\n# Stub\n- **X:** <todo>\n");
        let rules = vec![ui, wf, refr, stub_ref];

        let mut rep = CompileReport::default();
        let overrides: BTreeSet<String> = ["versioning".to_string()].into_iter().collect();
        let (inline, ptr) = compile_rules(&rules, &traits(&["always", "web-ui", "windows-app"]), &overrides, &mut rep);
        assert_eq!(inline, vec!["- **Scrollbars:** thin", "- **Git Branching:** main"]);
        assert_eq!(
            ptr,
            vec!["- **Building & Packaging:** Read `.brain/build.md` before packaging."]
        );
        assert_eq!(rep.overridden_topics, vec!["versioning"]);

        let (inline, _) = compile_rules(&rules, &traits(&["always", "desktop-gui"]), &BTreeSet::new(), &mut rep);
        assert_eq!(inline[0], "- **Chrome:** frameless");
        assert_eq!(inline[3], "- **Versioning:** later\n  continued");
    }

    #[test]
    fn compile_replaces_sections_and_marker_idempotently() {
        let agents = "# AGENTS.md — X\r\n\r\n## Commands\r\n- run\r\n\r\n## Personal Rules (Brain)\r\n- **Old:** gone\r\n\r\n## Repo Guardrails & Gotchas\r\n- **Git Branching:** repo wins\r\n\r\n## Done Criteria\r\n- tests\r\n\r\n<!-- brain-sync: aaaaaaaaaaaa -->\r\n";
        let rules = vec![
            parse_rule("workflow.md", "- **Git Branching:** main\n- **Commit:** plain\n"),
            parse_rule(
                "gh.md",
                "---\napply_when: always\nmode: reference\ntrigger: Before CI work\n---\n# GitHub Actions\n- **x:** y\n",
            ),
        ];
        let mut rep = CompileReport::default();
        let t = traits(&["always"]);
        let out = compile_agents_text(agents, &rules, &t, "<!-- brain-sync: bbbbbbbbbbbb repo: cccccccccccc -->", &mut rep);
        assert!(out.contains("## Personal Rules (Brain)\n- **Commit:** plain\n\n## Playbooks (Read Before Task)\n- **GitHub Actions:** Read `.brain/gh.md` before CI work.\n\n## Repo Guardrails"));
        assert!(!out.contains("Old"));
        assert!(!out.contains("aaaaaaaaaaaa"));
        assert!(out.ends_with("- tests\n\n<!-- brain-sync: bbbbbbbbbbbb repo: cccccccccccc -->\n"));
        let again = compile_agents_text(&out, &rules, &t, "<!-- brain-sync: bbbbbbbbbbbb repo: cccccccccccc -->", &mut rep);
        assert_eq!(out, again);
    }
}
