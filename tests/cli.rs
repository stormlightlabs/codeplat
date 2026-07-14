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
    }
}

#[test]
fn commands_parse_with_default_paths_and_emit_actionable_foundation_guidance() {
    let fixture = FixtureRepository::new();

    for arguments in [[].as_slice(), ["map"].as_slice()] {
        let output = fixture.run(arguments);
        let markdown = stdout(&output);

        assert!(output.status.success(), "command failed: {markdown}");
        assert!(output.stderr.is_empty());
        assert!(markdown.contains("Status: Foundation"));
        assert!(markdown.contains("not available in this build"));
        assert_plain_report(&markdown);
    }
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
    assert_eq!(
        stdout(&output),
        "# Setaryb map\n\
         \n\
         Schema version: 1\n\
         Scope: `.`\n\
         Status: Foundation\n\
         \n\
         ## Summary\n\
         \n\
         The map command contract and renderers are ready; source-map analysis will be added in a subsequent ticket.\n\
         \n\
         ## Limitations\n\
         \n\
         - Source-map analysis is not available in this build.\n"
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
