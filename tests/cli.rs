use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::Value;

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct FixtureRepository {
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

    for arguments in [
        [].as_slice(),
        ["map"].as_slice(),
        ["history"].as_slice(),
        ["history", "contributors"].as_slice(),
    ] {
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
    let fixture = FixtureRepository::new();
    let output = fixture.run(&["history", "contributors", "nested", "--json"]);
    let json = stdout(&output);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let value: Value = serde_json::from_str(&json).expect("valid JSON report");
    assert_eq!(value["command"]["name"], "history");
    assert_eq!(value["command"]["operation"], "contributors");
    assert_eq!(value["scope"]["selected_path"], "nested");
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
