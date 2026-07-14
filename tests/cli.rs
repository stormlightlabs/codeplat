use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct FixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

struct HistoryFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

struct MapFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

struct MixedMapFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl FixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "setaryb-cli-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");

        fs::create_dir_all(root.join(".git/objects")).expect("create fixture Git objects directory");
        fs::create_dir_all(root.join(".git/refs/heads")).expect("create fixture Git refs directory");
        fs::create_dir_all(&cache).expect("create fixture cache directory");
        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(
            root.join(".git/config"),
            b"[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
        );
        gix::open(&root).expect("open valid fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().expect("run setaryb fixture command")
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_setaryb"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command
    }
}

impl Drop for FixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

impl HistoryFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "setaryb-history-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");
        fs::create_dir_all(&root).expect("create history fixture repository");
        fs::create_dir_all(root.join("src")).expect("create history fixture source scope");
        fs::create_dir_all(&cache).expect("create history fixture cache");

        let repository = gix::init(&root).expect("initialize history fixture repository");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_secs() as i64;
        let day = 86_400;
        let initial_tree = write_tree(&repository, &[("legacy.txt", "legacy")]);
        let first = write_commit(
            &repository,
            initial_tree,
            &[],
            "Alice",
            "alice@example.com",
            now - 400 * day,
            "Initial import",
        );

        let second_tree = write_tree(
            &repository,
            &[("legacy.txt", "legacy"), ("src/lib.rs", "pub fn parse() {}")],
        );
        let second = write_commit(
            &repository,
            second_tree,
            &[first],
            "Bob",
            "bob@example.com",
            now - 200 * day,
            "Implement parser",
        );

        let third_tree = write_tree(
            &repository,
            &[("legacy.txt", "legacy"), ("src/lib.rs", "pub fn parse() { 1 }")],
        );
        let third = write_commit(
            &repository,
            third_tree,
            &[second],
            "Alice",
            "alice@example.com",
            now - 20 * day,
            "Fix parser bug",
        );

        let side_tree = write_tree(
            &repository,
            &[
                ("legacy.txt", "legacy"),
                ("src/lib.rs", "pub fn parse() { 1 }"),
                ("src/side.rs", "pub fn side() {}"),
            ],
        );
        let side = write_commit(
            &repository,
            side_tree,
            &[second],
            "Carol",
            "carol@example.com",
            now - 15 * day,
            "Emergency hotfix side work",
        );
        let merge = write_commit(
            &repository,
            third_tree,
            &[third, side],
            "Maintainer",
            "maintainer@example.com",
            now - 5 * day,
            "Merge side work",
        );

        let final_tree = write_tree(
            &repository,
            &[
                ("legacy.txt", "legacy"),
                ("src/lib.rs", "pub fn parse() { 1 }"),
                ("src/main.rs", "fn main() { 1 }"),
            ],
        );
        let final_commit = write_commit(
            &repository,
            final_tree,
            &[merge],
            "Bob",
            "bob@example.com",
            now - 2 * day,
            "Rollback entrypoint",
        );
        drop(repository);
        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(
            root.join(".git/refs/heads/main"),
            format!("{final_commit}\n").as_bytes(),
        );
        gix::open(&root).expect("open history fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_setaryb"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command.output().expect("run history fixture command")
    }
}

impl Drop for HistoryFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

impl MapFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "setaryb-map-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");
        fs::create_dir_all(root.join("src")).expect("create map fixture source scope");
        fs::create_dir_all(&cache).expect("create map fixture cache");

        let tracked_files = [
            (".gitignore", "src/ignored.rs\n"),
            ("README.md", "source map fixture\n"),
            ("src/lib.rs", "pub fn parse() {}\n"),
            ("src/tracked_ignored.rs", "pub fn tracked() {}\n"),
            ("src/one.rs", "pub fn duplicate() {}\n"),
            ("src/two.rs", "pub fn duplicate() {}\n"),
            ("src/use.rs", "fn use_it() { duplicate(); }\n"),
            ("src/broken.rs", "fn broken( {\n"),
        ];
        let repository = gix::init(&root).expect("initialize map fixture repository");
        let tree = write_tree(&repository, &tracked_files);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_secs() as i64;
        let commit = write_commit(
            &repository,
            tree,
            &[],
            "Map Fixture",
            "map@example.com",
            now,
            "Initial source map fixture",
        );
        drop(repository);

        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(root.join(".git/refs/heads/main"), format!("{commit}\n").as_bytes());
        for (path, contents) in tracked_files {
            write_file(root.join(path), contents.as_bytes());
        }
        write_file(
            root.join("src/lib.rs"),
            b"pub fn parse() { let value = 1; let _ = value; }\n",
        );
        write_file(root.join("src/untracked.rs"), b"pub fn fresh() {}\n");
        write_file(root.join("src/ignored.rs"), b"pub fn ignored() {}\n");
        gix::open(&root).expect("open map fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_setaryb"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command.output().expect("run map fixture command")
    }
}

impl Drop for MapFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

impl MixedMapFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "setaryb-mixed-map-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");
        fs::create_dir_all(root.join("src")).expect("create mixed map fixture source scope");
        fs::create_dir_all(&cache).expect("create mixed map fixture cache");

        let tracked_files = [
            (".gitignore", "src/ignored.js\n"),
            ("README.md", "mixed-language source map fixture\n"),
            ("src/lib.rs", "pub fn parse() { let value = 1; let _ = value; }\n"),
            ("src/broken.js", "export function broken( {\n"),
            (
                "src/module.js",
                "import { helper } from \"./helper.js\";\nexport function build(value) { return new Widget(value, helper); }\nexport class Widget { render() { return helper(); } }\n",
            ),
            (
                "src/types.ts",
                "export interface User { name: string; }\nexport class Service { run(user: User) { return user.name; } }\nexport function create(user: User): Service { return new Service(); }\n",
            ),
            (
                "src/component.tsx",
                "export function View(props: { label: string }) { return <button>{props.label}</button>; }\n",
            ),
            (
                "src/service.py",
                "from helpers import helper\n\nclass Service:\n    def run(self, value):\n        return helper(value)\n\ndef create(value):\n    return Service().run(value)\n",
            ),
            ("src/broken.py", "def broken(:\n    pass\n"),
            (
                "src/service.rb",
                "module Billing\n  class Service\n    def run(value)\n      helper(value)\n    end\n  end\nend\n\ndef build\n  Service.new\nend\n",
            ),
            ("src/broken.rb", "def broken(\nend\n"),
        ];
        let repository = gix::init(&root).expect("initialize mixed map fixture repository");
        let tree = write_tree(&repository, &tracked_files);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_secs() as i64;
        let commit = write_commit(
            &repository,
            tree,
            &[],
            "Mixed Map Fixture",
            "mixed@example.com",
            now,
            "Initial mixed-language source map fixture",
        );
        drop(repository);

        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(root.join(".git/refs/heads/main"), format!("{commit}\n").as_bytes());
        for (path, contents) in tracked_files {
            write_file(root.join(path), contents.as_bytes());
        }
        write_file(
            root.join("src/panel.jsx"),
            b"import React from \"react\";\nexport function Panel() { return <div />; }\n",
        );
        write_file(root.join("src/ignored.js"), b"export function ignored() {}\n");
        gix::open(&root).expect("open mixed map fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_setaryb"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command.output().expect("run mixed map fixture command")
    }
}

impl Drop for MixedMapFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

fn write_tree(repository: &gix::Repository, files: &[(&str, &str)]) -> gix::ObjectId {
    let mut root_entries = Vec::new();
    let mut source_entries = Vec::new();
    for (path, contents) in files {
        let blob = repository
            .write_object(gix::objs::Blob { data: contents.as_bytes().to_vec() })
            .expect("write fixture blob")
            .detach();
        if let Some(filename) = path.strip_prefix("src/") {
            source_entries.push(gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Blob.into(),
                filename: filename.into(),
                oid: blob,
            });
        } else {
            root_entries.push(gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Blob.into(),
                filename: (*path).into(),
                oid: blob,
            });
        }
    }
    if !source_entries.is_empty() {
        source_entries.sort();
        let source_tree = repository
            .write_object(gix::objs::Tree { entries: source_entries })
            .expect("write fixture source tree")
            .detach();
        root_entries.push(gix::objs::tree::Entry {
            mode: gix::objs::tree::EntryKind::Tree.into(),
            filename: "src".into(),
            oid: source_tree,
        });
    }
    root_entries.sort();
    repository
        .write_object(gix::objs::Tree { entries: root_entries })
        .expect("write fixture root tree")
        .detach()
}

fn write_commit(
    repository: &gix::Repository, tree: gix::ObjectId, parents: &[gix::ObjectId], name: &str, email: &str,
    seconds: i64, message: &str,
) -> gix::ObjectId {
    let timestamp = format!("{seconds} +0000");
    let signature = gix::actor::SignatureRef { name: name.into(), email: email.into(), time: &timestamp };
    repository
        .new_commit_as(signature, signature, message, tree, parents.iter().copied())
        .expect("write fixture commit")
        .id
}

fn write_file(path: impl AsRef<Path>, contents: &[u8]) {
    let mut file = File::create(path).expect("create fixture file");
    file.write_all(contents).expect("write fixture file");
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn assert_plain_report(output: &str) {
    assert!(
        !output.contains('\u{1b}'),
        "report stdout must not include ANSI escape sequences: {output:?}"
    );
    assert!(
        output
            .bytes()
            .all(|byte| { byte == b'\n' || byte == b'\r' || byte >= 0x20 })
    );
}

#[test]
fn root_map_and_history_help_are_complete() {
    let fixture = FixtureRepository::new();

    for arguments in [
        ["--help"].as_slice(),
        ["map", "--help"].as_slice(),
        ["history", "--help"].as_slice(),
    ] {
        let output = fixture.run(arguments);
        let help = stdout(&output);

        assert!(output.status.success(), "help failed: {help}");
        assert!(output.stderr.is_empty());
        assert!(help.contains("Usage:"));
        assert!(help.contains("Examples:"));
        assert!(help.contains("--format <FORMAT>"));
        assert!(help.contains("--json"));
        if arguments.first().copied() == Some("map") {
            assert!(help.contains("--exclude <GLOB>"));
        }
    }
}

#[test]
fn commands_parse_with_default_paths_and_emit_actionable_foundation_guidance() {
    let fixture = FixtureRepository::new();
    let output = fixture.run(&[]);
    let markdown = stdout(&output);

    assert!(output.status.success(), "command failed: {markdown}");
    assert!(output.stderr.is_empty());
    assert!(markdown.contains("Status: Foundation"));
    assert!(markdown.contains("not available in this build"));
    assert_plain_report(&markdown);
}

#[test]
fn history_without_commits_uses_the_analysis_exit_category() {
    let fixture = FixtureRepository::new();
    let output = fixture.run(&["history", "--json"]);

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("history analysis failed"));
}

#[test]
fn json_rendering_is_versioned_semantic_and_plain() {
    let fixture = FixtureRepository::new();
    let output = fixture.run(&["--json"]);
    let json = stdout(&output);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_plain_report(&json);

    let value: Value = serde_json::from_str(&json).expect("valid JSON report");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"]["name"], "briefing");
    assert_eq!(value["status"], "foundation");
}

#[test]
fn selected_paths_and_history_operations_are_preserved_in_the_typed_report() {
    let fixture = HistoryFixtureRepository::new();
    let output = fixture.run(&["history", "contributors", "src", "--json"]);
    let json = stdout(&output);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let value: Value = serde_json::from_str(&json).expect("valid JSON report");
    assert_eq!(value["command"]["name"], "history");
    assert_eq!(value["command"]["operation"], "contributors");
    assert_eq!(value["scope"]["selected_path"], "src");
    assert_eq!(
        value["history"]["contributors"]["overall"]
            .as_array()
            .expect("scoped contributors")
            .iter()
            .find(|contributor| contributor["email"] == "alice@example.com")
            .expect("scoped Alice contributor")["commits"],
        1
    );
}

#[test]
fn history_json_contains_all_signals_evidence_and_required_caveats() {
    let fixture = HistoryFixtureRepository::new();
    let output = fixture.run(&["history", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid history JSON");

    assert!(
        output.status.success(),
        "history failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_plain_report(&stdout(&output));
    assert_eq!(value["status"], "analyzed");
    assert_eq!(value["history"]["commits_seen"], 6);
    assert_eq!(value["history"]["non_merge_commits_seen"], 5);
    assert_eq!(value["history"]["settings"]["window_days"], 365);
    assert_eq!(value["history"]["settings"]["recent_window_days"], 180);

    let churn_paths: BTreeMap<_, _> = value["history"]["churn"]["paths"]
        .as_array()
        .expect("churn paths")
        .iter()
        .map(|path| {
            (
                path["path"].as_str().expect("path name").to_owned(),
                path["commits"].as_u64().expect("count"),
            )
        })
        .collect();
    assert_eq!(churn_paths.get("src/lib.rs"), Some(&3));
    assert_eq!(churn_paths.get("src/side.rs"), Some(&1));
    assert_eq!(churn_paths.get("src"), None);
    assert!(
        value["history"]["bugs"]["overlap_paths"]
            .as_array()
            .expect("bug overlap")
            .iter()
            .any(|path| path["path"] == "src/lib.rs")
    );
    assert_eq!(
        value["history"]["bugs"]["commits"]
            .as_array()
            .expect("bug evidence")
            .len(),
        2
    );
    assert_eq!(
        value["history"]["firefighting"]["commits"]
            .as_array()
            .expect("firefighting evidence")
            .len(),
        2
    );
    assert_eq!(
        value["history"]["activity"]["months"]
            .as_array()
            .expect("activity months")
            .iter()
            .map(|month| month["commits"].as_u64().unwrap())
            .sum::<u64>(),
        6
    );

    let caveats = value["history"]["bugs"]["caveats"].as_array().expect("bug caveats");
    assert!(
        caveats
            .iter()
            .any(|caveat| caveat.as_str().expect("caveat").contains("commit-message discipline"))
    );
    let contributor_caveats = value["history"]["contributors"]["caveats"]
        .as_array()
        .expect("contributor caveats");
    assert!(
        contributor_caveats
            .iter()
            .any(|caveat| caveat.as_str().expect("caveat").contains("Squash merges"))
    );
}

#[test]
fn focused_history_commands_support_scopes_and_explicit_overrides() {
    let fixture = HistoryFixtureRepository::new();
    let bugs = fixture.run(&[
        "history",
        "bugs",
        "--window-days",
        "30",
        "--bug-keyword",
        "parser",
        "--json",
    ]);
    let bugs_json: Value = serde_json::from_str(&stdout(&bugs)).expect("valid focused bug JSON");

    assert!(bugs.status.success());
    assert_eq!(bugs_json["history"]["settings"]["window_days"], 30);
    assert_eq!(bugs_json["history"]["settings"]["bug_keywords"][0], "parser");
    assert_eq!(bugs_json["history"]["bugs"]["keywords"][0], "parser");
    assert_eq!(bugs_json["history"]["bugs"]["paths"][0]["path"], "src/lib.rs");
    assert_eq!(bugs_json["history"]["bugs"]["overlap_paths"][0]["path"], "src/lib.rs");
    assert!(bugs_json["history"]["churn"].is_null());

    let keyword_miss = fixture.run(&["history", "bugs", "--bug-keyword", "not-a-keyword", "--json"]);
    let keyword_miss_json: Value = serde_json::from_str(&stdout(&keyword_miss)).expect("valid keyword-miss JSON");
    assert!(keyword_miss.status.success());
    assert!(
        keyword_miss_json["history"]["bugs"]["commits"]
            .as_array()
            .expect("keyword-miss commits")
            .is_empty()
    );
    assert!(
        keyword_miss_json["history"]["bugs"]["caveats"]
            .as_array()
            .expect("keyword-miss caveats")
            .iter()
            .any(|caveat| caveat
                .as_str()
                .expect("caveat")
                .contains("No bug-related commits matched"))
    );

    let scoped = fixture.run(&["history", "churn", "src", "--json"]);
    let scoped_json: Value = serde_json::from_str(&stdout(&scoped)).expect("valid scoped churn JSON");
    assert!(scoped.status.success());
    assert_eq!(scoped_json["history"]["scope_path"], "src");
    assert!(
        scoped_json["history"]["churn"]["paths"]
            .as_array()
            .expect("scoped paths")
            .iter()
            .all(|path| path["path"].as_str().expect("path").starts_with("src/"))
    );

    let activity = fixture.run(&["history", "activity"]);
    let activity_markdown = stdout(&activity);
    assert!(activity.status.success());
    assert!(activity_markdown.contains("Monthly activity"));
    assert!(!activity_markdown.contains("Churn hotspots"));
}

#[test]
fn every_history_operation_renders_in_markdown_and_json() {
    let fixture = HistoryFixtureRepository::new();
    for operation in ["history", "churn", "contributors", "bugs", "activity", "firefighting"] {
        let operation_arguments: Vec<&str> =
            if operation == "history" { vec!["history"] } else { vec!["history", operation] };
        let markdown = fixture.run(&operation_arguments);
        assert!(markdown.status.success(), "{operation} Markdown failed");
        assert!(markdown.stderr.is_empty());
        assert!(stdout(&markdown).contains("Status: Analyzed"));
        assert_plain_report(&stdout(&markdown));

        let mut json_arguments = operation_arguments;
        json_arguments.push("--json");
        let json = fixture.run(&json_arguments);
        assert!(json.status.success(), "{operation} JSON failed");
        assert!(json.stderr.is_empty());
        let value: Value = serde_json::from_str(&stdout(&json)).expect("valid operation JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["status"], "analyzed");
    }
}

#[test]
fn map_inventory_and_rust_findings_are_reported_semantically() {
    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&["map", "--json"]);
    let json = stdout(&output);
    let value: Value = serde_json::from_str(&json).expect("valid map JSON");

    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_plain_report(&json);
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["status"], "analyzed");
    assert_eq!(value["command"]["name"], "map");
    assert_eq!(value["map"]["query_pack"], "rust-v1");
    assert_eq!(value["map"]["inventory"]["tracked"], 8);
    assert_eq!(value["map"]["inventory"]["modified"], 1);
    assert_eq!(value["map"]["inventory"]["untracked"], 1);
    assert_eq!(value["map"]["inventory"]["analyzed"], 7);
    assert_eq!(value["map"]["inventory"]["omitted"], 3);

    let files = value["map"]["files"].as_array().expect("map files");
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "src/lib.rs")
            .expect("modified Rust file")["worktree_state"],
        "modified"
    );
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "src/tracked_ignored.rs")
            .expect("tracked ignored Rust file")["worktree_state"],
        "tracked"
    );
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "src/untracked.rs")
            .expect("untracked Rust file")["worktree_state"],
        "untracked"
    );
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "src/broken.rs")
            .expect("malformed Rust file")["status"],
        "partial"
    );
    assert!(
        files
            .iter()
            .find(|file| file["path"] == "src/lib.rs")
            .expect("parsed Rust file")["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .any(|symbol| symbol["name"] == "parse" && symbol["role"] == "definition")
    );

    let omissions = value["map"]["omissions"].as_array().expect("map omissions");
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "src/ignored.rs" && omission["reason"] == "ignored_untracked" })
    );
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "README.md" && omission["reason"] == "unsupported_language" })
    );

    let findings = value["map"]["findings"].as_array().expect("map findings");
    assert!(findings.iter().any(|finding| finding["kind"] == "parse_error"));
    assert!(
        findings
            .iter()
            .any(|finding| { finding["kind"] == "ambiguous_reference" && finding["path"] == "src/use.rs" })
    );
}

#[test]
fn map_scope_exclusions_and_markdown_limitations_are_preserved() {
    let fixture = MapFixtureRepository::new();
    let json_output = fixture.run(&["map", "src", "--exclude", "src/two.rs", "--json"]);
    let json: Value = serde_json::from_str(&stdout(&json_output)).expect("valid scoped map JSON");

    assert!(json_output.status.success());
    assert!(json_output.stderr.is_empty());
    assert_eq!(json["scope"]["selected_path"], "src");
    assert_eq!(json["map"]["scope_path"], "src");
    assert_eq!(json["map"]["exclusions"][0], "src/two.rs");
    assert!(
        json["map"]["files"]
            .as_array()
            .expect("scoped files")
            .iter()
            .all(|file| file["path"].as_str().expect("file path").starts_with("src/"))
    );
    assert!(
        json["map"]["omissions"]
            .as_array()
            .expect("scoped omissions")
            .iter()
            .any(|omission| { omission["path"] == "src/two.rs" && omission["reason"] == "explicit_exclusion" })
    );

    let markdown_output = fixture.run(&["map", "src"]);
    let markdown = stdout(&markdown_output);
    assert!(markdown_output.status.success());
    assert!(markdown_output.stderr.is_empty());
    assert_plain_report(&markdown);
    assert!(markdown.contains("Map scope: `src`"));
    assert!(markdown.contains("Inventory:"));
    assert!(markdown.contains("Map findings"));
    assert!(markdown.contains("Map limitations"));
    assert!(markdown.contains("lexically"));
}

#[test]
fn map_builds_ambiguous_edges_and_applies_focus_and_token_budget() {
    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&[
        "map",
        "--focus",
        "duplicate",
        "--focus-path",
        "src/one.rs",
        "--map-tokens",
        "40",
        "--no-cache",
        "--json",
    ]);
    let json: Value = serde_json::from_str(&stdout(&output)).expect("valid focused map JSON");

    assert!(
        output.status.success(),
        "focused map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(json["map"]["selection"]["token_budget"], 40);
    assert!(json["map"]["selection"]["estimated_tokens"].as_u64().unwrap() <= 40);
    assert_eq!(json["map"]["ranking"][0]["path"], "src/one.rs");
    assert_eq!(json["map"]["cache"]["status"], "disabled");
    assert!(json["map"]["edges"].as_array().unwrap().iter().any(|edge| {
        edge["source"] == "src/use.rs"
            && edge["symbol"] == "duplicate"
            && edge["ambiguous"] == true
            && (edge["target"] == "src/one.rs" || edge["target"] == "src/two.rs")
    }));

    let elided = fixture.run(&[
        "map",
        "--focus",
        "duplicate",
        "--map-tokens",
        "14",
        "--no-cache",
        "--json",
    ]);
    let elided_json: Value = serde_json::from_str(&stdout(&elided)).expect("valid elided map JSON");
    assert!(
        elided_json["map"]["selection"]["snippets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|snippet| snippet["truncated"] == true && snippet["symbol"]["location"]["start"]["line"] == 1)
    );
}

#[test]
fn map_cache_modes_hit_invalidate_refresh_and_disable_without_project_writes() {
    let fixture = MapFixtureRepository::new();
    let initial_cache_entries = fs::read_dir(&fixture.cache).expect("read empty cache root").count();
    assert_eq!(initial_cache_entries, 0);

    let disabled = fixture.run(&["map", "--no-cache", "--json"]);
    assert!(disabled.status.success());
    assert_eq!(
        fs::read_dir(&fixture.cache).expect("read disabled cache root").count(),
        0
    );

    let first = fixture.run(&["map", "--json"]);
    let first_json: Value = serde_json::from_str(&stdout(&first)).expect("first cached map JSON");
    assert_eq!(first_json["map"]["cache"]["status"], "refreshed");
    assert_eq!(first_json["map"]["cache"]["refreshed"].as_array().unwrap().len(), 7);

    let second = fixture.run(&["map", "--json"]);
    let second_json: Value = serde_json::from_str(&stdout(&second)).expect("cache-hit map JSON");
    assert_eq!(second_json["map"]["cache"]["status"], "hit");
    assert_eq!(second_json["map"]["cache"]["hits"], 7);
    assert_eq!(first_json["map"]["ranking"], second_json["map"]["ranking"]);
    assert_eq!(first_json["map"]["selection"], second_json["map"]["selection"]);

    let always = fixture.run(&["map", "--cache", "always", "--json"]);
    let always_json: Value = serde_json::from_str(&stdout(&always)).expect("always-refresh map JSON");
    assert_eq!(always_json["map"]["cache"]["status"], "refreshed");
    assert_eq!(always_json["map"]["cache"]["refreshed"].as_array().unwrap().len(), 7);

    write_file(
        fixture.root.join("src/lib.rs"),
        b"pub fn parse() { let changed = 2; let _ = changed; }\n",
    );
    let manual = fixture.run(&["map", "--cache", "manual", "--json"]);
    let manual_json: Value = serde_json::from_str(&stdout(&manual)).expect("manual stale map JSON");
    assert_eq!(manual_json["map"]["cache"]["status"], "stale");
    assert!(
        manual_json["map"]["cache"]["stale"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "src/lib.rs")
    );
    assert!(
        manual_json["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "src/lib.rs")
            .unwrap()["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|limitation| limitation.as_str().unwrap().contains("potentially stale"))
    );

    let files = fixture.run(&["map", "--cache", "files", "--cache-file", "src/lib.rs", "--json"]);
    let files_json: Value = serde_json::from_str(&stdout(&files)).expect("file-refresh map JSON");
    assert_eq!(files_json["map"]["cache"]["status"], "refreshed");
    assert_eq!(
        files_json["map"]["cache"]["refreshed"],
        serde_json::json!(["src/lib.rs"])
    );

    let missing_changed_file = fixture.run(&["map", "--cache", "files", "--json"]);
    assert_eq!(missing_changed_file.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_changed_file.stderr).contains("requires at least one"));
}

#[test]
fn mixed_language_map_is_explicit_deterministic_and_keeps_other_findings() {
    let fixture = MixedMapFixtureRepository::new();
    let first = fixture.run(&["map", "--no-cache", "--json"]);
    let second = fixture.run(&["map", "--no-cache", "--json"]);
    let first_stdout = stdout(&first);
    let second_stdout = stdout(&second);
    let json: Value = serde_json::from_str(&first_stdout).expect("valid mixed-language map JSON");

    assert!(
        first.status.success(),
        "mixed map failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "repeated mixed map failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());
    assert_plain_report(&first_stdout);
    assert_eq!(
        first_stdout, second_stdout,
        "mixed-language map ordering must be deterministic"
    );
    assert_eq!(json["map"]["query_pack"], "mixed");
    assert_eq!(json["map"]["query_packs"]["javascript"], "javascript-v1");
    assert_eq!(json["map"]["query_packs"]["javascript_jsx"], "javascript-v1");
    assert_eq!(json["map"]["query_packs"]["typescript"], "typescript-v1");
    assert_eq!(json["map"]["query_packs"]["typescript_tsx"], "typescript-v1");
    assert_eq!(json["map"]["query_packs"]["python"], "python-v1");
    assert_eq!(json["map"]["query_packs"]["ruby"], "ruby-v1");

    let files = json["map"]["files"].as_array().expect("mixed map files");
    for (path, language, extension) in [
        ("src/lib.rs", "rust", "rs"),
        ("src/module.js", "javascript", "js"),
        ("src/panel.jsx", "javascript_jsx", "jsx"),
        ("src/types.ts", "typescript", "ts"),
        ("src/component.tsx", "typescript_tsx", "tsx"),
        ("src/service.py", "python", "py"),
        ("src/service.rb", "ruby", "rb"),
    ] {
        let file = files
            .iter()
            .find(|file| file["path"] == path)
            .expect("language fixture file");
        assert_eq!(file["language"], language);
        assert_eq!(file["extension"], extension);
        assert_eq!(file["status"], "complete");
        assert!(!file["symbols"].as_array().expect("symbols").is_empty());
    }
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "src/broken.js")
            .expect("malformed JavaScript file")["status"],
        "partial"
    );
    assert!(
        files
            .iter()
            .find(|file| file["path"] == "src/module.js")
            .expect("JavaScript file")["symbols"]
            .as_array()
            .expect("JavaScript symbols")
            .iter()
            .any(|symbol| symbol["name"] == "Widget" && symbol["kind"] == "class")
    );
    assert!(
        files
            .iter()
            .find(|file| file["path"] == "src/types.ts")
            .expect("TypeScript file")["symbols"]
            .as_array()
            .expect("TypeScript symbols")
            .iter()
            .any(|symbol| symbol["name"] == "User" && symbol["kind"] == "interface")
    );
    let python = files
        .iter()
        .find(|file| file["path"] == "src/service.py")
        .expect("Python file");
    assert!(
        python["symbols"]
            .as_array()
            .expect("Python symbols")
            .iter()
            .any(|symbol| {
                symbol["name"] == "Service" && symbol["kind"] == "class" && symbol["role"] == "definition"
            })
    );
    assert!(
        python["symbols"]
            .as_array()
            .expect("Python symbols")
            .iter()
            .any(|symbol| {
                symbol["name"] == "run"
                    && symbol["kind"] == "function"
                    && symbol["role"] == "definition"
                    && symbol["scope"] == serde_json::json!(["Service"])
            })
    );
    assert!(
        python["symbols"]
            .as_array()
            .expect("Python symbols")
            .iter()
            .any(|symbol| { symbol["name"] == "helper" && symbol["role"] == "reference" })
    );
    let ruby = files
        .iter()
        .find(|file| file["path"] == "src/service.rb")
        .expect("Ruby file");
    assert!(
        ruby["symbols"].as_array().expect("Ruby symbols").iter().any(|symbol| {
            symbol["name"] == "Billing" && symbol["kind"] == "module" && symbol["role"] == "definition"
        })
    );
    assert!(ruby["symbols"].as_array().expect("Ruby symbols").iter().any(|symbol| {
        symbol["name"] == "run"
            && symbol["kind"] == "method"
            && symbol["role"] == "definition"
            && symbol["scope"] == serde_json::json!(["Billing", "Service"])
    }));
    assert!(
        ruby["symbols"]
            .as_array()
            .expect("Ruby symbols")
            .iter()
            .any(|symbol| { symbol["name"] == "Service" && symbol["role"] == "reference" })
    );
    for path in ["src/broken.py", "src/broken.rb"] {
        let file = files
            .iter()
            .find(|file| file["path"] == path)
            .expect("malformed dynamic-language file");
        assert_eq!(file["status"], "partial");
        assert!(!file["limitations"].as_array().expect("file limitations").is_empty());
    }

    let omissions = json["map"]["omissions"].as_array().expect("mixed omissions");
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "src/ignored.js" && omission["reason"] == "ignored_untracked" })
    );
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "README.md" && omission["reason"] == "unsupported_language" })
    );
    assert!(
        json["map"]["findings"]
            .as_array()
            .expect("mixed findings")
            .iter()
            .all(|finding| finding["kind"] != "query_error")
    );
    assert!(
        json["map"]["findings"]
            .as_array()
            .expect("mixed findings")
            .iter()
            .any(|finding| finding["kind"] == "parse_error" && finding["path"] == "src/broken.js")
    );

    let markdown = fixture.run(&["map"]);
    let markdown_stdout = stdout(&markdown);
    assert!(markdown.status.success());
    assert!(markdown.stderr.is_empty());
    assert!(markdown_stdout.contains("JavaScript files"));
    assert!(markdown_stdout.contains("JavaScript (JSX) files"));
    assert!(markdown_stdout.contains("TypeScript files"));
    assert!(markdown_stdout.contains("TypeScript (TSX) files"));
    assert!(markdown_stdout.contains("Python files"));
    assert!(markdown_stdout.contains("Ruby files"));
    assert!(markdown_stdout.contains("src/broken.py"));
    assert!(markdown_stdout.contains("Tree-sitter reported parse errors in this Python file"));
    assert!(markdown_stdout.contains("src/broken.rb"));
    assert!(markdown_stdout.contains("Tree-sitter reported parse errors in this Ruby file"));
    assert!(markdown_stdout.contains("query-pack provenance"));
    assert_plain_report(&markdown_stdout);
}

#[test]
fn format_json_and_json_alias_share_the_report_renderer() {
    let fixture = FixtureRepository::new();
    let format_output = fixture.run(&["--format", "json"]);
    let alias_output = fixture.run(&["--json"]);

    assert!(format_output.status.success());
    assert!(alias_output.status.success());
    assert!(format_output.stderr.is_empty());
    assert!(alias_output.stderr.is_empty());

    let format_json: Value = serde_json::from_str(&stdout(&format_output)).expect("format JSON");
    let alias_json: Value = serde_json::from_str(&stdout(&alias_output)).expect("alias JSON");
    assert_eq!(format_json, alias_json);
}

#[test]
fn markdown_snapshot_is_direct_and_readable() {
    let fixture = FixtureRepository::new();
    let output = fixture.run(&["map"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stable_markdown = stdout(&output)
        .lines()
        .filter(|line| !line.starts_with("Repository: `"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        stable_markdown,
        "# Setaryb map\n\
         \n\
         Schema version: 1\n\
         Scope: `.`\n\
         Status: Analyzed\n\
         \n\
         ## Summary\n\
         \n\
         Analyzed 0 Rust source files and recorded 0 omitted paths within the selected source scope.\n\
         \n\
         ## Source map\n\
         \n\
         Map scope: `.`\n\
         Query pack: `rust-v1`\n\
         Inventory: 0 tracked (0 modified), 0 untracked, 0 analyzed, 0 omitted\n\
         \n\
         ### Rust files\n\
         \n\
         No Rust files were analyzed.\n\
         \n\
         ### Map limitations\n\
         \n\
         - Rust definitions and references are extracted lexically; imports, types, macros, and runtime behavior are not resolved.\n\
         - Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge.\n\
         - Tracked files are eligible even when ignore rules match them; ignored untracked files are omitted and recorded.\n"
    );
}

#[test]
fn color_options_never_change_json_stdout() {
    let fixture = FixtureRepository::new();
    let never = fixture.run(&["--color", "never", "--json"]);
    let always = fixture.run(&["--color", "always", "--json"]);

    assert!(never.status.success());
    assert!(always.status.success());
    assert!(never.stderr.is_empty());
    assert!(always.stderr.is_empty());
    assert_eq!(stdout(&never), stdout(&always));
    assert_plain_report(&stdout(&always));
}

#[test]
fn automatic_diagnostic_color_honors_no_color() {
    let fixture = FixtureRepository::new();
    let no_color = fixture
        .command(&["--format", "markdown", "--json"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run no-color fixture command");
    let always = fixture.run(&["--color", "always", "--format", "markdown", "--json"]);

    assert_eq!(no_color.status.code(), Some(2));
    assert!(no_color.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&no_color.stderr).contains('\u{1b}'));

    assert_eq!(always.status.code(), Some(2));
    assert!(always.stdout.is_empty());
    assert!(String::from_utf8_lossy(&always.stderr).contains('\u{1b}'));
}

#[test]
fn parser_and_usage_errors_use_the_documented_exit_category_and_stderr() {
    let fixture = FixtureRepository::new();
    let invalid_value = fixture.run(&["--format", "xml"]);
    let conflicting_output = fixture.run(&["--format", "markdown", "--json"]);

    assert_eq!(invalid_value.status.code(), Some(2));
    assert!(invalid_value.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid_value.stderr).contains("error:"));

    assert_eq!(conflicting_output.status.code(), Some(2));
    assert!(conflicting_output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&conflicting_output.stderr).contains("cannot be combined"));
}
