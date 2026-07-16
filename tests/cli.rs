use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
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

impl FixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "codeplat-cli-{}-{}",
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
        self.command(arguments).output().expect("run codeplat fixture command")
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
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

struct HistoryFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl HistoryFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "codeplat-history-{}-{}",
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
            "Robert Alias",
            "ALIAS@example.com",
            now - 200 * day,
            "Implement fixture prefix debug parser",
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
                (".mailmap", "Bob <bob@example.com> Robert Alias <alias@example.com>\n"),
                ("legacy.txt", "legacy"),
                ("src/binary.rs", "\0binary"),
                ("src/empty.rs", ""),
                ("src/generated.rs", "// generated file\npub fn generated() {}"),
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
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
            "codeplat-map-{}-{}",
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
        self.command(arguments).output().expect("run map fixture command")
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command
    }
}

struct MapFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl Drop for MapFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

struct ClassificationFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl ClassificationFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "codeplat-classification-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");
        fs::create_dir_all(root.join("src/vendor")).expect("create classification fixture source scope");
        fs::create_dir_all(root.join("vendor")).expect("create classification fixture vendor scope");
        fs::create_dir_all(root.join("target/debug/deps")).expect("create classification fixture build output");
        fs::create_dir_all(&cache).expect("create classification fixture cache");

        let large_generated = format!(
            "// Code generated by fixture. DO NOT EDIT.\npub fn generated() {{ {} }}\n",
            "let _ = 1; ".repeat(100_000)
        );
        let tracked_files = vec![
            (".gitignore", "target/\nvendor/\n".to_owned()),
            ("README.md", "classification fixture\n".to_owned()),
            (
                "Cargo.toml",
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n".to_owned(),
            ),
            ("src/lib.rs", "pub fn maintained() {}\n".to_owned()),
            ("src/main.rs", "fn main() { fixture::maintained(); }\n".to_owned()),
            (
                "src/generated_marker.rs",
                "// Code generated by fixture. DO NOT EDIT.\npub fn marker() {}\n".to_owned(),
            ),
            ("src/bundle.min.js", format!("const bundle={};", "x".repeat(1_200))),
            ("src/heuristic.js", format!("const heuristic={};", "x".repeat(1_200))),
            (
                "src/bundle.js.map",
                "{\"version\":3,\"sources\":[\"src/lib.ts\"]}\n".to_owned(),
            ),
            (
                "src/generated_parser.rs",
                "pub fn generated_parser() { let generated = 1; let _ = generated; }\n".to_owned(),
            ),
            ("src/vendor/tracked.rs", "pub fn vendored() {}\n".to_owned()),
            ("src/large.generated.rs", large_generated),
        ];
        let repository = gix::init(&root).expect("initialize classification fixture repository");
        let tree_files = tracked_files
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect::<Vec<_>>();
        let tree = write_tree(&repository, &tree_files);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_secs() as i64;
        let commit = write_commit(
            &repository,
            tree,
            &[],
            "Classification Fixture",
            "classification@example.com",
            now,
            "Initial classification fixture",
        );
        drop(repository);
        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(root.join(".git/refs/heads/main"), format!("{commit}\n").as_bytes());
        for (path, contents) in &tracked_files {
            write_file(root.join(path), contents.as_bytes());
        }
        write_file(root.join("vendor/untracked.rs"), b"pub fn ignored_vendored() {}\n");
        for index in 0..256 {
            write_file(
                root.join(format!("target/debug/deps/artifact-{index:03}.json")),
                b"{}\n",
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("artifact-000.json", root.join("target/debug/deps/artifact-link.json"))
            .expect("create ignored build-output symlink");
        gix::open(&root).expect("open classification fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command.output().expect("run classification fixture command")
    }
}

impl Drop for ClassificationFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

struct MixedMapFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl MixedMapFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "codeplat-mixed-map-{}-{}",
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
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

struct JavaCSharpMapFixtureRepository {
    root: PathBuf,
    cache: PathBuf,
    temporary_root: PathBuf,
}

impl JavaCSharpMapFixtureRepository {
    fn new() -> Self {
        let suffix = format!(
            "codeplat-java-csharp-map-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temporary_root = env::temp_dir().join(suffix);
        let root = temporary_root.join("repository");
        let cache = temporary_root.join("xdg-cache");
        fs::create_dir_all(root.join("src")).expect("create Java and C# map fixture source scope");
        fs::create_dir_all(&cache).expect("create Java and C# map fixture cache");

        let tracked_files = [
            (".gitignore", "src/ignored.java\n"),
            ("README.md", "Java and C# source map fixture\n"),
            ("src/lib.rs", "pub fn parse() {}\n"),
            (
                "src/service.java",
                "package example;\nimport java.util.List;\n\npublic class Service extends BaseService {\n    private class Hidden {}\n\n    public Result run(Input input) {\n        return new Result(input.value());\n    }\n}\n\ninterface Runner {}\n",
            ),
            (
                "src/consumer.java",
                "package consumer;\n\nclass Consumer {\n    Service make() {\n        return new Service();\n    }\n}\n",
            ),
            (
                "src/service.cs",
                "using System;\n\nnamespace Example.App {\n    public class Service : BaseService, IRunner {\n        private class Hidden {}\n        private Helper helper;\n\n        public Result Run(Input input) {\n            helper.Execute(input);\n            return new Result();\n        }\n    }\n\n    public struct Value {}\n    public interface IRunner {}\n}\n",
            ),
            ("src/broken.cs", "namespace Broken {\n    public class Broken( {\n"),
        ];
        let repository = gix::init(&root).expect("initialize Java and C# map fixture repository");
        let tree = write_tree(&repository, &tracked_files);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_secs() as i64;
        let commit = write_commit(
            &repository,
            tree,
            &[],
            "JVM and CLR Fixture",
            "languages@example.com",
            now,
            "Initial Java and C# source map fixture",
        );
        drop(repository);

        write_file(root.join(".git/HEAD"), b"ref: refs/heads/main\n");
        write_file(root.join(".git/refs/heads/main"), format!("{commit}\n").as_bytes());
        for (path, contents) in tracked_files {
            write_file(root.join(path), contents.as_bytes());
        }
        write_file(root.join("src/untracked.java"), b"package fresh; class Fresh {}\n");
        write_file(root.join("src/ignored.java"), b"package ignored; class Ignored {}\n");
        gix::open(&root).expect("open Java and C# map fixture repository");

        Self { root, cache, temporary_root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeplat"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("XDG_CACHE_HOME", &self.cache);
        command.output().expect("run Java and C# map fixture command")
    }
}

impl Drop for JavaCSharpMapFixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temporary_root);
    }
}

fn write_tree(repository: &gix::Repository, files: &[(&str, &str)]) -> gix::ObjectId {
    #[derive(Default)]
    struct TreeNode {
        files: Vec<(String, gix::ObjectId)>,
        directories: BTreeMap<String, TreeNode>,
    }

    fn insert_file(node: &mut TreeNode, path: &str, blob: gix::ObjectId) {
        let mut components = path.split('/');
        let Some(first) = components.next() else {
            return;
        };
        let rest = components.collect::<Vec<_>>();
        if rest.is_empty() {
            node.files.push((first.to_owned(), blob));
        } else {
            insert_file(
                node.directories.entry(first.to_owned()).or_default(),
                &rest.join("/"),
                blob,
            );
        }
    }

    fn write_node(repository: &gix::Repository, node: TreeNode) -> gix::ObjectId {
        let mut entries = node
            .files
            .into_iter()
            .map(|(filename, oid)| gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Blob.into(),
                filename: filename.into(),
                oid,
            })
            .collect::<Vec<_>>();
        for (filename, child) in node.directories {
            entries.push(gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Tree.into(),
                filename: filename.into(),
                oid: write_node(repository, child),
            });
        }
        entries.sort();
        repository
            .write_object(gix::objs::Tree { entries })
            .expect("write fixture tree")
            .detach()
    }

    let mut root = TreeNode::default();
    for (path, contents) in files {
        let blob = repository
            .write_object(gix::objs::Blob { data: contents.as_bytes().to_vec() })
            .expect("write fixture blob")
            .detach();
        insert_file(&mut root, path, blob);
    }
    write_node(repository, root)
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

fn cache_json_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
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
        assert!(help.contains("Usage: codeplat"));
        assert!(help.contains("Examples:"));
        assert!(help.contains("--format <FORMAT>"));
        assert!(help.contains("--json"));
        assert!(help.contains("github.com/stormlightlabs/codeplat/issues"));
        if arguments.first().copied() == Some("map") {
            assert!(help.contains("--exclude <GLOB>"));
        }
    }
}

#[test]
fn default_command_combines_history_and_ranked_source_map() {
    let fixture = MixedMapFixtureRepository::new();
    let output = fixture.run(&[
        "--no-cache",
        "--focus",
        "Service",
        "--focus-path",
        "src",
        "--map-tokens",
        "120",
        "--json",
    ]);
    let json = stdout(&output);
    let value: Value = serde_json::from_str(&json).expect("valid integrated briefing JSON");

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_plain_report(&json);
    assert_eq!(value["command"]["name"], "briefing");
    assert_eq!(value["profile"], "compact");
    assert_eq!(value["status"], "analyzed");
    assert!(
        value["summary"]
            .as_str()
            .expect("briefing summary")
            .contains("reachable commits")
    );
    assert!(value["history"]["churn"].is_object());
    assert!(value["history"]["contributors"].is_object());
    assert!(value["history"]["bugs"].is_object());
    assert!(value["history"]["activity"].is_object());
    assert!(value["history"]["firefighting"].is_object());
    let recommendations = value["reading_plan"]["recommendations"]
        .as_array()
        .expect("briefing reading plan");
    assert!(recommendations.len() <= 10, "recommendations: {recommendations:?}");
    if recommendations.len() < 5 {
        assert!(value["reading_plan"]["shortfall"].is_object());
    }
    let paths = recommendations
        .iter()
        .map(|recommendation| recommendation["path"].as_str().expect("recommendation path"))
        .collect::<BTreeSet<_>>();
    assert_eq!(paths.len(), recommendations.len(), "reading-plan paths must be unique");
    for (index, recommendation) in recommendations.iter().enumerate() {
        assert_eq!(recommendation["ordinal"], index as u64 + 1);
        assert!(matches!(
            recommendation["purpose"].as_str(),
            Some("start_here" | "architecture" | "runtime" | "tests" | "supporting_context")
        ));
        assert!(!recommendation["reason"].as_str().unwrap_or_default().is_empty());
        assert!(!recommendation["evidence_kinds"].as_array().unwrap().is_empty());
        assert!(recommendation["confidence"].is_string());
    }
    assert!(recommendations.iter().any(|recommendation| {
        recommendation["evidence_kinds"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "focus"))
    }));
    assert_eq!(value["map"]["query_pack"], "mixed");
    assert_eq!(value["map"]["cache"]["status"], "disabled");
    assert_eq!(value["map"]["selection"]["token_budget"], 120);
    assert!(value["map"]["selection"]["estimated_tokens"].as_u64().unwrap() <= 120);
    for collection in [
        "files",
        "symbols",
        "omissions",
        "findings",
        "edges",
        "ranking",
        "snippets",
    ] {
        let summary = &value["map"]["collections"][collection];
        assert!(summary["total"].is_u64());
        assert!(summary["returned"].as_u64().unwrap() <= summary["total"].as_u64().unwrap());
        assert!(summary["truncated"].is_boolean());
        if summary["truncated"] == true {
            assert!(summary["reason"].is_string());
        }
    }
    assert!(value["map"]["query_packs"]["javascript"].is_string());
    assert!(value["map"]["query_packs"]["typescript"].is_string());
}

#[test]
fn default_markdown_briefing_keeps_history_and_map_sections_readable() {
    let fixture = MixedMapFixtureRepository::new();
    let output = fixture.run(&["--no-cache"]);
    let markdown = stdout(&output);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_plain_report(&markdown);
    assert!(markdown.starts_with("# Codeplat briefing\n"));
    assert!(markdown.contains("Status: Analyzed"));
    for section in ["## History analysis", "### History observations"] {
        assert!(markdown.contains(section), "missing Markdown section: {section}");
    }
    assert!(!markdown.contains("## Source map"));
    assert!(!markdown.contains("### Ranked map selection"));
    assert!(!markdown.contains("### Generated, vendor, and minified paths"));
    assert!(
        markdown.lines().count() < 100,
        "briefing was {} lines",
        markdown.lines().count()
    );
    assert!(markdown.contains("Bug keywords: `fix`, `bug`, `broken`"));
    assert!(markdown.contains("## Repository overview"));
    assert!(markdown.contains("## Reading plan"));
    assert!(markdown.contains("### start_here"));
    assert!(markdown.find("## Repository overview").unwrap() < markdown.find("## Reading plan").unwrap());
    assert!(markdown.find("## Reading plan").unwrap() < markdown.find("## History analysis").unwrap());
    let json = fixture.run(&["--no-cache", "--json"]);
    let json_value: Value = serde_json::from_slice(&json.stdout).expect("default briefing JSON");
    let default_recommendations = json_value["reading_plan"]["recommendations"]
        .as_array()
        .expect("default reading plan recommendations");
    assert!(
        (5..=10).contains(&default_recommendations.len()),
        "default recommendations: {default_recommendations:?}"
    );
    assert!(!markdown.contains("\\`, \\`"));
}

#[test]
fn default_briefing_history_is_concise_while_detailed_modes_remain_available() {
    let fixture = HistoryFixtureRepository::new();
    let concise = fixture.run(&["--no-cache"]);
    let concise_markdown = stdout(&concise);

    assert!(concise.status.success());
    assert!(concise.stderr.is_empty());
    assert!(concise_markdown.contains("### History observations"));
    assert!(!concise_markdown.contains("### Churn hotspots"));
    assert!(!concise_markdown.contains("### Contributor concentration"));
    assert!(!concise_markdown.contains("### Monthly activity"));
    assert!(!concise_markdown.contains("#### Evidence commits"));
    let observation_count = concise_markdown.lines().filter(|line| line.starts_with("- **")).count();
    assert!(observation_count <= 5, "observations: {observation_count}");
    assert!(concise_markdown.find("## Reading plan").unwrap() < concise_markdown.find("## History analysis").unwrap());

    let json = fixture.run(&["--no-cache", "--json"]);
    let value: Value = serde_json::from_slice(&json.stdout).expect("valid concise briefing JSON");
    let observations = value["history"]["observations"]
        .as_array()
        .expect("history observations");
    assert!(observations.len() <= 5);
    assert!(observations.iter().all(|observation| observation["kind"].is_string()));

    let focused = fixture.run(&["--no-cache", "history"]);
    let focused_markdown = stdout(&focused);
    assert!(focused.status.success());
    assert!(focused_markdown.contains("### Churn hotspots"));
    assert!(focused_markdown.contains("### Contributor concentration"));
    assert!(focused_markdown.contains("### Monthly activity"));
    assert!(focused_markdown.contains("#### Evidence commits"));

    let evidence = fixture.run(&["--profile", "evidence", "--no-cache"]);
    let evidence_markdown = stdout(&evidence);
    assert!(evidence.status.success());
    assert!(evidence_markdown.contains("### Churn hotspots"));
    assert!(evidence_markdown.contains("### Monthly activity"));
}

#[test]
fn briefing_prioritizes_root_manifest_and_conventional_entry_points() {
    let fixture = ClassificationFixtureRepository::new();
    let output = fixture.run(&["--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid entry-point briefing JSON");
    assert!(output.status.success());

    let paths = value["reading_plan"]["recommendations"]
        .as_array()
        .expect("reading plan")
        .iter()
        .map(|recommendation| recommendation["path"].as_str().expect("recommendation path"))
        .collect::<Vec<_>>();
    let position = |path: &str| {
        paths
            .iter()
            .position(|candidate| *candidate == path)
            .expect("recommended path")
    };
    assert!(position("README.md") < position("Cargo.toml"), "paths: {paths:?}");
    assert!(position("Cargo.toml") < position("src/lib.rs"), "paths: {paths:?}");
    assert!(position("src/lib.rs") < position("src/main.rs"), "paths: {paths:?}");
}

#[test]
fn compact_recovery_command_succeeds_without_including_classified_trees() {
    let fixture = MixedMapFixtureRepository::new();
    fs::create_dir_all(fixture.root.join("target/debug/deps")).expect("create ignored build tree");
    for index in 0..256 {
        write_file(
            fixture.root.join(format!("target/debug/deps/artifact-{index:03}.json")),
            b"{}\n",
        );
    }

    let compact = fixture.run(&["--no-cache"]);
    let markdown = stdout(&compact);
    assert!(compact.status.success());
    assert!(markdown.contains("Next useful command: `codeplat map --profile evidence`."));

    let evidence = fixture.run(&["map", "--profile", "evidence", "--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&evidence)).expect("valid evidence recovery JSON");
    assert!(
        evidence.status.success(),
        "evidence recovery failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    assert_eq!(value["quality"]["resource_limited"], false);
    assert!(value["map"]["inventory"]["omitted"].as_u64().unwrap() < 64);
    assert!(
        !value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"].as_str().is_some_and(|path| path.starts_with("target/")))
    );
}

#[test]
fn evidence_profile_is_explicit_and_reports_collection_totals() {
    let fixture = MixedMapFixtureRepository::new();
    let output = fixture.run(&["map", "--profile", "evidence", "--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid evidence profile JSON");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(value["profile"], "evidence");
    assert_eq!(value["map"]["profile"], "evidence");
    assert!(value["map"]["collections"]["files"]["returned"].as_u64().unwrap() > 0);
    assert!(
        value["map"]["collections"]["files"]["returned"].as_u64().unwrap()
            <= value["map"]["collections"]["files"]["total"].as_u64().unwrap()
    );
}

#[test]
fn resource_bound_source_inputs_are_partial_and_typed() {
    let fixture = FixtureRepository::new();
    fs::create_dir_all(fixture.root.join("src")).expect("create source fixture directory");
    let oversized = vec![b'x'; 1_048_577];
    write_file(fixture.root.join("src/oversized.rs"), &oversized);
    write_file(fixture.root.join("src/binary.rs"), b"fn binary() {\0 }\n");

    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid bounded resource JSON");
    let omissions = value["map"]["omissions"].as_array().expect("map omissions");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(value["limits"]["max_file_bytes"], 1_048_576);
    assert!(omissions.iter().any(|omission| omission["reason"] == "oversized"));
    assert!(omissions.iter().any(|omission| omission["reason"] == "binary"));
    assert!(value["map"]["collections"]["omissions"]["total"].as_u64().unwrap() >= 2);
    assert!(stdout(&output).len() < 8 * 1_024 * 1_024);
}

#[test]
fn map_reports_bounded_landmarks_project_roots_and_recursive_boundaries() {
    let fixture = FixtureRepository::new();
    for directory in [
        "packages/app/src",
        "packages/app/tests",
        "packages/python",
        "packages/ruby",
        "packages/java",
        "packages/dotnet",
        ".github/workflows",
        "vendor/submodule",
        "nested-repo/src",
    ] {
        fs::create_dir_all(fixture.root.join(directory)).expect("create topology fixture directory");
    }
    write_file(fixture.root.join("README.md"), b"topology fixture\n");
    write_file(fixture.root.join("AGENTS.md"), b"agent instructions\n");
    write_file(fixture.root.join("CONTRIBUTING.md"), b"contributor instructions\n");
    write_file(
        fixture.root.join("Cargo.toml"),
        b"[workspace]\nmembers = [\"packages/app\"]\n",
    );
    write_file(fixture.root.join("Cargo.lock"), b"version = 3\n");
    write_file(fixture.root.join("Makefile"), b"all:\n\ttrue\n");
    write_file(fixture.root.join("CODEOWNERS"), b"* @maintainers\n");
    write_file(fixture.root.join("LICENSE"), b"license\n");
    write_file(fixture.root.join(".github/workflows/ci.yml"), b"name: CI\n");
    write_file(
        fixture.root.join(".gitmodules"),
        b"[submodule \"vendor/submodule\"]\n\tpath = vendor/submodule\n\turl = https://example.invalid/submodule\n",
    );
    write_file(fixture.root.join("packages/app/package.json"), br#"{"name":"app"}"#);
    write_file(
        fixture.root.join("packages/python/pyproject.toml"),
        b"[project]\nname = \"python\"\n",
    );
    write_file(
        fixture.root.join("packages/ruby/Gemfile"),
        b"source \"https://rubygems.org\"\n",
    );
    write_file(fixture.root.join("packages/java/pom.xml"), b"<project></project>\n");
    write_file(
        fixture.root.join("packages/dotnet/app.csproj"),
        b"<Project></Project>\n",
    );
    write_file(fixture.root.join("packages/app/src/lib.rs"), b"pub fn app() {}\n");
    write_file(
        fixture.root.join("packages/app/tests/app.rs"),
        b"#[test]\nfn app() {}\n",
    );
    write_file(
        fixture.root.join("vendor/submodule/.git"),
        b"gitdir: ../.git/modules/submodule\n",
    );
    write_file(
        fixture.root.join("nested-repo/.git"),
        b"gitdir: ../.git/worktrees/nested\n",
    );
    write_file(fixture.root.join("nested-repo/src/lib.rs"), b"pub fn nested() {}\n");
    let repository = gix::open(&fixture.root).expect("open topology fixture for tracked instruction file");
    let tree = write_tree(
        &repository,
        &[
            ("AGENTS.md", "agent instructions\n"),
            ("Cargo.toml", "[workspace]\nmembers = [\"packages/app\"]\n"),
        ],
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after the Unix epoch")
        .as_secs() as i64;
    let commit = write_commit(
        &repository,
        tree,
        &[],
        "Topology Fixture",
        "topology@example.com",
        now,
        "Topology fixture",
    );
    drop(repository);
    write_file(fixture.root.join(".git/HEAD"), b"ref: refs/heads/main\n");
    write_file(
        fixture.root.join(".git/refs/heads/main"),
        format!("{commit}\n").as_bytes(),
    );

    let output = fixture.run(&["map", "--no-cache", "--focus-path", "packages/app", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid topology JSON");
    assert!(output.status.success(), "topology map failed: {:?}", output.stderr);
    assert!(output.stderr.is_empty(), "topology stderr: {:?}", output.stderr);
    assert!(
        value["map"]["landmarks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|landmark| { landmark["kind"] == "agent_instructions" && landmark["path"] == "AGENTS.md" })
    );
    assert!(
        value["map"]["landmarks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|landmark| { landmark["kind"] == "submodule" && landmark["path"] == "vendor/submodule" })
    );
    assert!(
        value["map"]["landmarks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|landmark| { landmark["kind"] == "nested_repository" && landmark["path"] == "nested-repo" })
    );
    assert!(
        value["map"]["project_roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|root| { root["path"] == "packages/app" && root["kind"] == "package" })
    );
    assert!(
        value["map"]["project_roots"].as_array().unwrap().iter().any(|root| {
            root["path"] == "packages/app"
                && root["recommended_paths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|path| path == "packages/app/src/lib.rs")
        }),
        "project roots: {}; landmarks: {}",
        value["map"]["project_roots"],
        value["map"]["landmarks"]
    );
    for collection in ["landmarks", "project_roots"] {
        let summary = &value["map"]["collections"][collection];
        assert!(summary["returned"].as_u64().unwrap() <= summary["total"].as_u64().unwrap());
        assert!(summary["truncated"].is_boolean());
    }
    assert!(
        !value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| { file["path"] == "nested-repo/src/lib.rs" })
    );

    let recursive = fixture.run(&["map", "--recursive", "--no-cache", "--json"]);
    let recursive_value: Value = serde_json::from_str(&stdout(&recursive)).expect("valid recursive topology JSON");
    assert!(
        recursive.status.success(),
        "recursive map failed: {:?}",
        recursive.stderr
    );
    assert!(
        recursive_value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| { file["path"] == "nested-repo/src/lib.rs" })
    );

    let briefing = fixture.run(&["--no-cache", "--focus-path", "packages/app", "--json"]);
    let briefing_value: Value = serde_json::from_slice(&briefing.stdout).expect("valid topology briefing JSON");
    let recommendations = briefing_value["reading_plan"]["recommendations"]
        .as_array()
        .expect("topology reading plan");
    assert!(recommendations.iter().any(|recommendation| {
        recommendation["project_root"] == "packages/app"
            && recommendation["evidence_kinds"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "focus"))
    }));
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
    let fixture = MixedMapFixtureRepository::new();
    let output = fixture.run(&["--no-cache", "--json"]);
    let json = stdout(&output);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_plain_report(&json);

    let value: Value = serde_json::from_str(&json).expect("valid JSON report");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"]["name"], "briefing");
    assert_eq!(value["status"], "analyzed");
    assert!(value["history"].is_object());
    assert!(value["map"].is_object());
}

#[test]
fn machine_report_provenance_is_typed_and_repeated_runs_are_comparable() {
    let fixture = MapFixtureRepository::new();
    let first = fixture.run(&["map", "--no-cache", "--json"]);
    let second = fixture.run(&["map", "--no-cache", "--json"]);
    let first_json = stdout(&first);
    let value: Value = serde_json::from_str(&first_json).expect("valid provenance report");

    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first_json, stdout(&second));
    assert_eq!(value["provenance"]["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["provenance"]["effective_options"]["format"], "json");
    assert_eq!(value["provenance"]["repository"]["object_format"], "sha1");
    assert!(
        value["provenance"]["repository"]["stable_id"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(value["provenance"]["head"]["reference"], "refs/heads/main");
    assert!(value["provenance"]["head"]["oid"].as_str().unwrap().len() >= 40);
    assert!(value["provenance"]["captured_at"].as_str().unwrap().contains('T'));
    assert_eq!(value["provenance"]["cache"]["status"], "disabled");
    assert!(value["provenance"]["languages"]["rust"]["grammar_version"].is_string());
    assert_eq!(value["provenance"]["worktree"]["state"], "mixed");
}

#[test]
fn capabilities_are_available_without_repository_analysis() {
    let fixture = FixtureRepository::new();
    let output = fixture.run(&["capabilities", "--json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid capabilities JSON");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["report_kind"], "capabilities");
    assert_eq!(value["query_packs_valid"], true);
    assert_eq!(value["limits"]["compact"]["max_files"], 4_096);
    assert!(
        value["languages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|language| language["language"] == "java")
    );
}

#[test]
fn doctor_reports_support_health_without_source_evidence_or_repository_mutation() {
    let fixture = FixtureRepository::new();
    let before = fs::read(fixture.root.join(".git/HEAD")).expect("read HEAD before doctor");
    let output = fixture.run(&["doctor", "--json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid doctor JSON");

    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(value["report_kind"], "doctor");
    assert_eq!(value["source_evidence_collected"], false);
    assert_eq!(value["repository_state_changed"], false);
    assert!(
        value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "path_safety")
    );
    assert!(
        value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "query_packs")
    );
    assert_eq!(
        fs::read(fixture.root.join(".git/HEAD")).expect("read HEAD after doctor"),
        before
    );
    assert!(!stdout(&output).contains("pub fn"));
}

#[test]
fn strict_policy_renders_typed_partial_report_then_returns_analysis_failure() {
    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&["map", "--strict", "--no-cache", "--json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("strict mode still emits JSON");

    assert_eq!(output.status.code(), Some(5));
    assert_eq!(value["quality"]["partial"], true);
    assert!(
        value["quality"]["strict_issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "partial")
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("strict report policy rejected"));
}

#[test]
fn strict_quality_uses_complete_counts_when_compact_samples_are_truncated() {
    let fixture = FixtureRepository::new();
    for index in 0..8 {
        write_file(fixture.root.join(format!("a{index}.rs")), b"\0binary");
    }
    write_file(fixture.root.join("z.go"), b"package unsupported\n");

    let output = fixture.run(&["map", "--strict", "--no-cache", "--json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("strict compact report is valid JSON");

    assert_eq!(output.status.code(), Some(5));
    assert_eq!(value["map"]["availability"]["unsupported_paths"], 1);
    assert_eq!(value["quality"]["unsupported"], true);
    assert!(
        !value["map"]["omissions"]
            .as_array()
            .expect("bounded omission sample")
            .iter()
            .any(|omission| omission["reason"] == "unsupported_language")
    );
}

#[test]
fn compact_projection_is_reported_without_becoming_actionable_quality() {
    let fixture = FixtureRepository::new();
    fs::create_dir_all(fixture.root.join("src")).expect("create projection source directory");
    for index in 0..40 {
        write_file(
            fixture.root.join(format!("src/file{index}.rs")),
            format!("pub fn file{index}() {{}}\n").as_bytes(),
        );
    }

    let output = fixture.run(&["map", "--strict", "--no-cache", "--json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("compact projection JSON");

    assert!(
        output.status.success(),
        "projection should not fail strict policy: {:?}",
        output.stderr
    );
    assert!(output.stderr.is_empty());
    assert_eq!(value["quality"]["projection"], true);
    assert_eq!(value["quality"]["truncated"], true);
    assert_eq!(value["quality"]["resource_limited"], false);
    assert_eq!(value["quality"]["strict_issues"].as_array().unwrap().len(), 0);
    assert_eq!(value["map"]["collections"]["files"]["reason"], "profile_projection");
}

#[test]
fn irrelevant_unsupported_source_does_not_poison_a_briefing_but_focus_does() {
    let fixture = ClassificationFixtureRepository::new();
    write_file(fixture.root.join("src/unsupported.go"), b"package unsupported\n");

    let briefing = fixture.run(&["--strict", "--no-cache", "--json"]);
    let briefing_value: Value = serde_json::from_slice(&briefing.stdout).expect("briefing JSON");
    assert!(
        briefing.status.success(),
        "irrelevant unsupported source: {:?}",
        briefing.stderr
    );
    assert_eq!(briefing_value["quality"]["unsupported"], false);
    assert_eq!(briefing_value["map"]["availability"]["unsupported_paths"], 1);
    assert!(
        briefing_value["map"]["availability"]["unsupported_path_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "src/unsupported.go")
    );

    let focused = fixture.run(&[
        "map",
        "--strict",
        "--focus-path",
        "src/unsupported.go",
        "--no-cache",
        "--json",
    ]);
    let focused_value: Value = serde_json::from_slice(&focused.stdout).expect("focused JSON");
    assert_eq!(focused.status.code(), Some(5));
    assert_eq!(focused_value["quality"]["unsupported"], true);
    assert!(
        focused_value["quality"]["strict_issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "unsupported")
    );
}

#[test]
fn history_completeness_marks_shallow_and_missing_objects_and_strict_rejects_them() {
    let shallow = HistoryFixtureRepository::new();
    let head = {
        let repository = gix::open(&shallow.root).expect("open shallow fixture");
        repository.head_id().expect("resolve shallow HEAD").to_string()
    };
    write_file(shallow.root.join(".git/shallow"), format!("{head}\n").as_bytes());

    let shallow_output = shallow.run(&["history", "--json"]);
    let shallow_value: Value = serde_json::from_slice(&shallow_output.stdout).expect("valid shallow JSON");
    assert!(shallow_output.status.success());
    assert_eq!(
        shallow_value["provenance"]["history"]["completeness"]["status"],
        "shallow"
    );
    assert_eq!(shallow_value["quality"]["incomplete"], true);

    let missing = HistoryFixtureRepository::new();
    let repository = gix::open(&missing.root).expect("open missing-object fixture");
    let head = repository.head_id().expect("resolve missing-object HEAD");
    let parent = repository
        .find_commit(head)
        .expect("read missing-object HEAD")
        .parent_ids()
        .next()
        .expect("fixture HEAD has a parent");
    let parent_text = parent.to_string();
    let parent_path = missing
        .root
        .join(".git/objects")
        .join(&parent_text[..2])
        .join(&parent_text[2..]);
    drop(repository);
    assert!(parent_path.is_file(), "fixture parent should be a loose object");
    fs::remove_file(parent_path).expect("remove one reachable Git object");

    let missing_output = missing.run(&["history", "--strict", "--json"]);
    let missing_value: Value = serde_json::from_slice(&missing_output.stdout).expect("valid missing-object JSON");
    assert_eq!(missing_output.status.code(), Some(5));
    assert_eq!(
        missing_value["provenance"]["history"]["completeness"]["status"],
        "missing_objects"
    );
    assert_eq!(missing_value["quality"]["incomplete"], true);
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("strict report policy rejected"));
}

#[test]
fn selected_paths_and_history_operations_are_preserved_in_the_typed_report() {
    let fixture = HistoryFixtureRepository::new();
    let output = fixture.run(&["history", "contributors", "src", "--include-emails", "--json"]);
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
        1
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
fn scoped_history_activity_and_envelope_counts_only_include_affected_commits() {
    let fixture = HistoryFixtureRepository::new();
    let output = fixture.run(&["history", "activity", "src", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid scoped activity JSON");

    assert!(output.status.success());
    assert_eq!(value["history"]["scope_path"], "src");
    assert_eq!(value["history"]["commits_seen"], 4);
    assert_eq!(value["history"]["non_merge_commits_seen"], 4);
    assert_eq!(
        value["history"]["activity"]["months"]
            .as_array()
            .expect("scoped activity months")
            .iter()
            .map(|month| month["commits"].as_u64().expect("monthly commit count"))
            .sum::<u64>(),
        4
    );
}

#[test]
fn contributors_apply_committed_mailmap_and_hide_emails_by_default() {
    let fixture = HistoryFixtureRepository::new();
    let compact = fixture.run(&["history", "contributors", "--json"]);
    let compact: Value = serde_json::from_str(&stdout(&compact)).expect("valid compact contributor JSON");
    let contributors = &compact["history"]["contributors"];

    assert_eq!(contributors["mailmap_applied"], true);
    let bob = contributors["overall"]
        .as_array()
        .expect("overall contributors")
        .iter()
        .find(|contributor| contributor["name"] == "Bob")
        .expect("mailmap canonicalized Bob");
    assert_eq!(bob["commits"], 2);
    assert!(bob.get("email").is_none());
    let mapping = &contributors["identity_mappings"][0];
    assert_eq!(mapping["raw_name"], "Robert Alias");
    assert_eq!(mapping["canonical_name"], "Bob");
    assert!(mapping.get("raw_email").is_none());

    let disclosed = fixture.run(&["history", "contributors", "--include-emails", "--json"]);
    let disclosed: Value = serde_json::from_str(&stdout(&disclosed)).expect("valid disclosed contributor JSON");
    let mapping = &disclosed["history"]["contributors"]["identity_mappings"][0];
    assert_eq!(mapping["raw_email"], "ALIAS@example.com");
    assert_eq!(mapping["canonical_email"], "bob@example.com");
}

#[test]
fn history_keywords_are_word_aware_and_record_matches_with_substring_compatibility() {
    let fixture = HistoryFixtureRepository::new();
    let word = fixture.run(&["history", "bugs", "--json"]);
    let word: Value = serde_json::from_str(&stdout(&word)).expect("valid word-aware bug JSON");
    let commits = word["history"]["bugs"]["commits"]
        .as_array()
        .expect("word-aware commits");
    assert_eq!(word["history"]["bugs"]["keyword_match"], "word");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["matched_terms"], serde_json::json!(["bug", "fix"]));
    assert!(
        commits
            .iter()
            .all(|commit| commit["subject"] != "Implement fixture prefix debug parser")
    );
    assert!(
        commits
            .iter()
            .all(|commit| commit["subject"] != "Emergency hotfix side work")
    );

    let substring = fixture.run(&["history", "bugs", "--keyword-match", "substring", "--json"]);
    let substring: Value = serde_json::from_str(&stdout(&substring)).expect("valid substring bug JSON");
    let commits = substring["history"]["bugs"]["commits"]
        .as_array()
        .expect("substring commits");
    assert_eq!(commits.len(), 3);
    assert!(commits.iter().any(|commit| {
        commit["subject"] == "Implement fixture prefix debug parser"
            && commit["matched_terms"] == serde_json::json!(["bug", "fix"])
    }));
}

#[test]
fn churn_reports_normalization_edge_cases_and_rename_unavailability() {
    let fixture = HistoryFixtureRepository::new();
    let output = fixture.run(&["history", "churn", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid churn JSON");
    let churn = &value["history"]["churn"];
    assert_eq!(churn["size_basis"], "current_head_blob_bytes");
    assert_eq!(churn["rename_continuity"]["status"], "unavailable");

    let paths = churn["paths"].as_array().expect("churn paths");
    let path = |name: &str| {
        paths
            .iter()
            .find(|path| path["path"] == name)
            .unwrap_or_else(|| panic!("missing churn path {name}"))
    };
    assert_eq!(path("src/lib.rs")["size_status"], "text");
    assert!(path("src/lib.rs")["commits_per_kib_milli"].is_number());
    assert_eq!(path("src/generated.rs")["size_status"], "generated");
    assert!(path("src/generated.rs")["commits_per_kib_milli"].is_number());
    assert_eq!(path("src/empty.rs")["size_status"], "empty");
    assert!(path("src/empty.rs").get("commits_per_kib_milli").is_none());
    assert_eq!(path("src/binary.rs")["size_status"], "binary");
    assert!(path("src/binary.rs").get("commits_per_kib_milli").is_none());
    assert_eq!(path("src/side.rs")["size_status"], "missing_at_head");
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
    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let json = stdout(&output);
    let value: Value = serde_json::from_str(&json).expect("valid map JSON");

    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(value["reading_plan"].is_null());
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
            .any(|omission| { omission["path"] == "README.md" && omission["reason"] == "non_source" })
    );

    let findings = value["map"]["findings"].as_array().expect("map findings");
    assert!(findings.iter().any(|finding| finding["kind"] == "parse_error"));
    assert!(
        !findings
            .iter()
            .any(|finding| { finding["kind"] == "ambiguous_reference" && finding["path"] == "src/use.rs" })
    );
}

#[test]
fn compact_classification_excludes_low_value_source_without_poisoning_quality() {
    let fixture = ClassificationFixtureRepository::new();
    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid classification JSON");

    assert!(
        output.status.success(),
        "classification map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(value["quality"]["partial"], false);
    assert_eq!(value["quality"]["unsupported"], false);
    assert_eq!(value["quality"]["resource_limited"], false);
    assert_eq!(value["quality"]["unsafe_paths"], false);
    assert!(
        value["map"]["classifications"]["total"].as_u64().unwrap() >= 7,
        "classifications: {}",
        value["map"]["classifications"]
    );
    assert!(value["map"]["classifications"]["generated"].as_u64().unwrap() >= 2);
    assert!(value["map"]["classifications"]["vendor"].as_u64().unwrap() >= 2);
    assert!(value["map"]["classifications"]["minified"].as_u64().unwrap() >= 2);
    assert!(value["map"]["classifications"]["source_map"].as_u64().unwrap() >= 1);
    assert!(
        value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "src/generated_parser.rs")
    );
    assert!(
        !value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "src/large.generated.rs")
    );
    assert!(
        value["map"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| omission["reason"] == "classified")
    );
    assert!(value["map"]["inventory"]["omitted"].as_u64().unwrap() < 32);

    let repeat = fixture.run(&["map", "--no-cache", "--json"]);
    let repeat_value: Value = serde_json::from_str(&stdout(&repeat)).expect("valid repeated classification JSON");
    assert_eq!(value["map"]["classifications"], repeat_value["map"]["classifications"]);

    let markdown = fixture.run(&["map", "--no-cache"]);
    assert!(markdown.status.success());
    assert!(stdout(&markdown).contains("excluded before parsing"));
    assert!(stdout(&markdown).contains("bounded_minification_heuristic"));
}

#[test]
fn classification_overrides_include_safe_text_and_keep_hard_limits() {
    let fixture = ClassificationFixtureRepository::new();

    let focused = fixture.run(&["map", "--no-cache", "--focus-path", "src/vendor/tracked.rs", "--json"]);
    let focused_value: Value = serde_json::from_str(&stdout(&focused)).expect("valid focused classification JSON");
    assert!(focused.status.success());
    let focused_file = focused_value["map"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/vendor/tracked.rs")
        .expect("focused vendored file");
    assert_eq!(focused_file["classification_overridden"], true);
    assert_eq!(focused_file["classifications"][0]["kind"], "vendor");

    let evidence = fixture.run(&["map", "--profile", "evidence", "--no-cache", "--json"]);
    let evidence_value: Value = serde_json::from_str(&stdout(&evidence)).expect("valid evidence classification JSON");
    assert!(evidence.status.success());
    assert!(
        !evidence_value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "src/generated_marker.rs" && file["classification_overridden"] == true)
    );
    assert!(
        !evidence_value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "vendor/untracked.rs" && file["classification_overridden"] == true)
    );

    let focused_untracked = fixture.run(&[
        "map",
        "--profile",
        "evidence",
        "--no-cache",
        "--focus-path",
        "vendor/untracked.rs",
        "--json",
    ]);
    let focused_untracked_value: Value =
        serde_json::from_str(&stdout(&focused_untracked)).expect("valid focused untracked classification JSON");
    assert!(focused_untracked.status.success());
    assert!(
        focused_untracked_value["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "vendor/untracked.rs" && file["classification_overridden"] == true),
        "focused map: {}",
        focused_untracked_value["map"]
    );

    let hard_limit = fixture.run(&["map", "--no-cache", "--focus-path", "src/large.generated.rs", "--json"]);
    let hard_limit_value: Value = serde_json::from_str(&stdout(&hard_limit)).expect("valid hard-limit JSON");
    assert!(hard_limit.status.success());
    assert!(
        hard_limit_value["map"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| {
                omission["path"] == "src/large.generated.rs"
                    && omission["reason"] == "oversized"
                    && omission["classification_overridden"] == true
            })
    );
}

#[test]
fn hidden_untracked_sources_are_included_but_hidden_ignored_sources_are_recorded() {
    let fixture = MapFixtureRepository::new();
    write_file(
        fixture.root.join(".gitignore"),
        b"src/ignored.rs\nsrc/.ignored-hidden.rs\n",
    );
    write_file(fixture.root.join("src/.hidden.rs"), b"pub fn hidden() {}\n");
    write_file(
        fixture.root.join("src/.ignored-hidden.rs"),
        b"pub fn ignored_hidden() {}\n",
    );

    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("valid hidden-file map JSON");

    assert!(output.status.success());
    assert!(
        value["map"]["files"]
            .as_array()
            .expect("map files")
            .iter()
            .any(|file| file["path"] == "src/.hidden.rs" && file["worktree_state"] == "untracked")
    );
    assert!(
        value["map"]["omissions"]
            .as_array()
            .expect("map omissions")
            .iter()
            .any(|omission| {
                omission["path"] == "src/.ignored-hidden.rs" && omission["reason"] == "ignored_untracked"
            })
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
fn map_rejects_unqualified_cross_file_edges_and_applies_focus_and_token_budget() {
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
    assert!(
        !json["map"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| { edge["source"] == "src/use.rs" && edge["symbol"] == "duplicate" })
    );

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
fn explain_reports_typed_focus_graph_history_and_omission_evidence() {
    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&["explain", "duplicate", "--focus", "duplicate", "--no-cache", "--json"]);
    assert!(
        output.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: Value = serde_json::from_str(&stdout(&output)).expect("valid explain JSON");
    assert_eq!(json["command"]["name"], "explain");
    assert_eq!(json["command"]["target"], "duplicate");
    assert_eq!(json["explain"]["target_kind"], "symbol");
    assert!(json["explain"]["matched_paths"].as_array().unwrap().len() >= 2);
    assert!(
        json["explain"]["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|limitation| {
                limitation
                    .as_str()
                    .unwrap_or_default()
                    .contains("not a semantic call graph")
            })
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

    write_file(
        fixture.root.join("src/lib.rs"),
        b"pub fn refreshed() { let changed = 3; let _ = changed; }\n",
    );
    let auto = fixture.run(&["map", "--json"]);
    let auto_json: Value = serde_json::from_str(&stdout(&auto)).expect("auto-refresh map JSON");
    assert_eq!(auto_json["map"]["cache"]["status"], "refreshed");
    assert_eq!(
        auto_json["map"]["cache"]["refreshed"],
        serde_json::json!(["src/lib.rs"])
    );
    let auto_lib = auto_json["map"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/lib.rs")
        .expect("refreshed Rust file");
    assert!(
        auto_lib["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| { symbol["name"] == "refreshed" && symbol["role"] == "definition" })
    );

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
fn files_cache_mode_refreshes_only_exact_requested_paths_and_reports_unavailable_files() {
    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&[
        "map",
        "--cache",
        "files",
        "--cache-file",
        "src/lib.rs",
        "--cache-file",
        "lib.rs",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "files cache failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid files cache JSON");
    let cache = &json["map"]["cache"];
    assert_eq!(cache["matched"], 1);
    assert_eq!(cache["unmatched"], 1);
    assert_eq!(cache["unavailable"], 6);
    assert_eq!(cache["hits"], 0);
    assert_eq!(cache["misses"], 6);
    assert_eq!(cache["refreshed"], serde_json::json!(["src/lib.rs"]));
    assert_eq!(json["map"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(json["map"]["files"][0]["path"], "src/lib.rs");
    assert!(
        json["map"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| { omission["reason"] == "cache_unavailable" && omission["path"] == "src/one.rs" })
    );
}

#[test]
fn files_cache_mode_does_not_match_duplicate_basenames() {
    let fixture = MapFixtureRepository::new();
    fs::create_dir_all(fixture.root.join("src/a")).expect("create first duplicate-basename directory");
    fs::create_dir_all(fixture.root.join("src/b")).expect("create second duplicate-basename directory");
    write_file(fixture.root.join("src/a/shared.rs"), b"pub fn first_shared() {}\n");
    write_file(fixture.root.join("src/b/shared.rs"), b"pub fn second_shared() {}\n");

    let output = fixture.run(&[
        "map",
        "--cache",
        "files",
        "--cache-file",
        "src/a/shared.rs",
        "--cache-file",
        "shared.rs",
        "--json",
    ]);
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid duplicate-basename cache JSON");
    assert_eq!(json["map"]["cache"]["matched"], 1);
    assert_eq!(json["map"]["cache"]["unmatched"], 1);
    assert_eq!(
        json["map"]["cache"]["refreshed"],
        serde_json::json!(["src/a/shared.rs"])
    );
    assert!(json["map"]["files"].as_array().unwrap().iter().any(|file| {
        file["path"] == "src/a/shared.rs"
            && file["symbols"]
                .as_array()
                .unwrap()
                .iter()
                .any(|symbol| symbol["name"] == "first_shared")
    }));
    assert!(
        !json["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| { file["path"] == "src/b/shared.rs" })
    );
}

#[test]
fn cache_records_are_reused_across_report_scopes_and_manual_uses_the_newest_record() {
    let fixture = MapFixtureRepository::new();
    let initial = fixture.run(&["map", "--json"]);
    assert!(initial.status.success());

    let scoped = fixture.run(&["map", "src", "--json"]);
    assert!(
        scoped.status.success(),
        "scoped map failed: {}",
        String::from_utf8_lossy(&scoped.stderr)
    );
    let scoped_json: Value = serde_json::from_slice(&scoped.stdout).expect("valid scoped map JSON");
    assert_eq!(scoped_json["map"]["scope_path"], "src");
    assert_eq!(scoped_json["map"]["cache"]["status"], "hit");
    assert_eq!(scoped_json["map"]["cache"]["hits"], 7);

    write_file(fixture.root.join("src/lib.rs"), b"pub fn newest_cached() {}\n");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let refreshed = fixture.run(&["map", "--json"]);
    assert!(refreshed.status.success());
    write_file(fixture.root.join("src/lib.rs"), b"pub fn current_not_cached() {}\n");
    let manual = fixture.run(&["map", "--cache", "manual", "--json"]);
    assert!(
        manual.status.success(),
        "manual map failed: {}",
        String::from_utf8_lossy(&manual.stderr)
    );
    let manual_json: Value = serde_json::from_slice(&manual.stdout).expect("valid manual map JSON");
    let lib = manual_json["map"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/lib.rs")
        .expect("manual cached lib file");
    assert!(
        lib["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| { symbol["name"] == "newest_cached" && symbol["role"] == "definition" })
    );
    assert!(
        !lib["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| { symbol["name"] == "current_not_cached" })
    );
    assert_eq!(manual_json["map"]["cache"]["status"], "stale");
}

#[test]
fn corrupt_cache_record_refreshes_and_cache_controls_do_not_touch_the_repository() {
    let fixture = MapFixtureRepository::new();
    let initial = fixture.run(&["map", "--json"]);
    assert!(initial.status.success());
    let records = cache_json_files(&fixture.cache.join("codeplat"));
    assert_eq!(records.len(), 7);
    write_file(&records[0], b"not valid JSON\n");

    let refreshed = fixture.run(&["map", "--json"]);
    assert!(
        refreshed.status.success(),
        "refresh failed: {}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let refreshed_json: Value = serde_json::from_slice(&refreshed.stdout).expect("valid refresh JSON");
    assert_eq!(refreshed_json["map"]["cache"]["status"], "refreshed");
    assert_eq!(refreshed_json["map"]["cache"]["misses"], 1);
    assert_eq!(refreshed_json["map"]["cache"]["refreshed"].as_array().unwrap().len(), 1);

    let source_before = fs::read(fixture.root.join("src/lib.rs")).expect("read source before cache control");
    let status = fixture.run(&["cache", "status", "--json"]);
    assert!(
        status.status.success(),
        "cache status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status_json: Value = serde_json::from_slice(&status.stdout).expect("valid cache status JSON");
    assert_eq!(status_json["records"], 7);
    assert_eq!(status_json["repositories"], 1);
    assert!(status_json["path"].as_str().unwrap().ends_with("codeplat"));

    let path = fixture.run(&["cache", "path", "--json"]);
    assert!(path.status.success());
    let path_json: Value = serde_json::from_slice(&path.stdout).expect("valid cache path JSON");
    assert_eq!(path_json["operation"], "path");

    let outside_cache_file = fixture.temporary_root.join("outside-cache.json");
    write_file(&outside_cache_file, b"must remain outside the cache root\n");
    let prune = fixture.run(&["cache", "prune", "--json"]);
    assert!(
        prune.status.success(),
        "cache prune failed: {}",
        String::from_utf8_lossy(&prune.stderr)
    );
    assert!(
        outside_cache_file.exists(),
        "cache prune crossed the configured cache root"
    );

    let clear = fixture.run(&["cache", "clear", "--json"]);
    assert!(
        clear.status.success(),
        "cache clear failed: {}",
        String::from_utf8_lossy(&clear.stderr)
    );
    let clear_json: Value = serde_json::from_slice(&clear.stdout).expect("valid cache clear JSON");
    assert_eq!(clear_json["removed_records"], 7);
    assert_eq!(clear_json["records"], 0);
    assert_eq!(
        fs::read(fixture.root.join("src/lib.rs")).expect("read source after cache control"),
        source_before
    );
}

#[cfg(unix)]
#[test]
fn cache_directories_and_records_are_user_private() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MapFixtureRepository::new();
    let output = fixture.run(&["map", "--json"]);
    assert!(output.status.success());
    let cache_root = fixture.cache.join("codeplat");
    assert_eq!(fs::metadata(&cache_root).unwrap().permissions().mode() & 0o777, 0o700);
    for record in cache_json_files(&cache_root) {
        assert_eq!(fs::metadata(record).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn concurrent_cache_writers_leave_only_complete_json_records() {
    let fixture = MapFixtureRepository::new();
    let children = (0..4)
        .map(|_| {
            fixture
                .command(&["map", "--cache", "always", "--json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn concurrent cache writer")
        })
        .collect::<Vec<_>>();
    for child in children {
        let output = child.wait_with_output().expect("wait for concurrent cache writer");
        assert!(
            output.status.success(),
            "concurrent cache writer failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "concurrent map output is invalid JSON ({error}); stdout={} bytes, stderr={}",
                output.stdout.len(),
                String::from_utf8_lossy(&output.stderr)
            )
        });
    }

    let records = cache_json_files(&fixture.cache.join("codeplat"));
    assert_eq!(records.len(), 7);
    for record in records {
        let bytes = fs::read(record).expect("read concurrent cache record");
        serde_json::from_slice::<Value>(&bytes).expect("concurrent cache record is complete JSON");
    }
}

#[cfg(unix)]
#[test]
fn hostile_worktree_symlink_is_omitted_without_reading_or_caching_target_content() {
    use std::os::unix::fs::symlink;

    let fixture = MapFixtureRepository::new();
    let outside = fixture.temporary_root.join("outside.rs");
    write_file(&outside, b"pub fn outside_secret() {}\n");
    symlink(&outside, fixture.root.join("src/outside.rs")).expect("create hostile source symlink");

    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid hostile-worktree JSON");
    assert!(
        output.status.success(),
        "hostile map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout(&output).contains("outside_secret"));
    assert!(
        json["map"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["path"] != "src/outside.rs")
    );
    assert!(
        json["map"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| omission["path"] == "src/outside.rs" && omission["reason"] == "symlink")
    );
    assert_eq!(
        fs::read_dir(&fixture.cache).unwrap().count(),
        0,
        "no-cache must not write cache data"
    );
}

#[cfg(unix)]
#[test]
fn worktree_swap_race_never_emits_content_from_a_replaced_directory() {
    use std::os::unix::fs::symlink;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;

    let fixture = MapFixtureRepository::new();
    let outside = fixture.temporary_root.join("race-outside");
    fs::create_dir_all(&outside).expect("create race target directory");
    write_file(outside.join("race.rs"), b"pub fn race_outside_secret() {}\n");
    write_file(fixture.root.join("src/race.rs"), b"pub fn race_inside() {}\n");

    let running = Arc::new(AtomicBool::new(true));
    let attacker_running = Arc::clone(&running);
    let source = fixture.root.join("src");
    let moved = fixture.root.join("src-real");
    let link_target = outside.clone();
    let attacker = thread::spawn(move || {
        while attacker_running.load(Ordering::Acquire) {
            if fs::rename(&source, &moved).is_ok() {
                if symlink(&link_target, &source).is_ok() {
                    thread::yield_now();
                    let _ = fs::remove_file(&source);
                }
                let _ = fs::rename(&moved, &source);
            }
        }
    });

    let output = fixture.run(&["map", "--no-cache", "--json"]);
    running.store(false, Ordering::Release);
    attacker.join().expect("join worktree swap fixture");

    assert!(
        output.status.success(),
        "swap-race map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("race_outside_secret"));
}

#[test]
fn malformed_tree_path_is_rejected_before_source_read_or_cache_write() {
    let fixture = MapFixtureRepository::new();
    let repository = gix::open(&fixture.root).expect("open malformed-tree fixture repository");
    let blob = repository
        .write_object(gix::objs::Blob { data: b"pub fn outside() {}\n".to_vec() })
        .expect("write malformed-tree blob")
        .detach();
    let tree = repository
        .write_object(gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Blob.into(),
                filename: "../outside.rs".into(),
                oid: blob,
            }],
        })
        .expect("write malformed tree")
        .detach();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after the Unix epoch")
        .as_secs() as i64;
    let commit = write_commit(
        &repository,
        tree,
        &[],
        "Malformed Tree Fixture",
        "malformed@example.com",
        now,
        "Malformed path fixture",
    );
    drop(repository);
    write_file(fixture.root.join(".git/HEAD"), b"ref: refs/heads/main\n");
    write_file(
        fixture.root.join(".git/refs/heads/main"),
        format!("{commit}\n").as_bytes(),
    );

    let output = fixture.run(&["map", "--json"]);
    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&output.stderr).contains("safety"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("outside"));
    assert_eq!(fs::read_dir(&fixture.cache).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn cache_root_symlink_into_repository_is_rejected_before_any_write() {
    use std::os::unix::fs::symlink;

    let fixture = MapFixtureRepository::new();
    let cache_target = fixture.root.join("cache-target");
    fs::create_dir_all(&cache_target).expect("create cache target");
    let cache_link = fixture.root.join("cache-link");
    symlink(&cache_target, &cache_link).expect("create cache-root symlink");

    let output = fixture
        .command(&["map", "--json"])
        .env("XDG_CACHE_HOME", &cache_link)
        .output()
        .expect("run cache containment fixture");
    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cache root"));
    assert_eq!(
        fs::read_dir(&cache_target).unwrap().count(),
        0,
        "cache writes must not cross a symlink"
    );
}

#[cfg(unix)]
#[test]
fn repository_filter_configuration_and_attributes_never_execute_a_sentinel() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MapFixtureRepository::new();
    let marker = fixture.temporary_root.join("filter-ran");
    let sentinel = fixture.temporary_root.join("filter-sentinel.sh");
    write_file(
        &sentinel,
        format!("#!/bin/sh\nprintf ran >> '{}'\n", marker.display()).as_bytes(),
    );
    let mut permissions = fs::metadata(&sentinel).expect("sentinel metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&sentinel, permissions).expect("make sentinel executable");
    write_file(fixture.root.join(".gitattributes"), b"src/*.rs filter=hostile\n");
    write_file(
        fixture.root.join(".git/config"),
        format!(
            "[core]\n\trepositoryformatversion = 0\n\tbare = false\n[filter \"hostile\"]\n\tprocess = {}\n\tclean = {}\n\tsmudge = {}\n",
            sentinel.display(),
            sentinel.display(),
            sentinel.display()
        )
        .as_bytes(),
    );

    let output = fixture.run(&["map", "--json"]);
    assert!(
        output.status.success(),
        "filter fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists(), "repository-controlled filter sentinel executed");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn non_utf8_worktree_paths_are_typed_omissions_and_never_become_lossy_output() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let fixture = MapFixtureRepository::new();
    let invalid_name = OsString::from_vec(b"bad\xff.rs".to_vec());
    write_file(
        fixture.root.join("src").join(invalid_name),
        b"pub fn hidden_outside() {}\n",
    );

    let output = fixture.run(&["map", "--no-cache", "--json"]);
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid non-UTF-8 path JSON");
    assert!(
        output.status.success(),
        "non-UTF-8 fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout(&output).contains("hidden_outside"));
    assert!(
        json["map"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| omission["reason"] == "unsafe_path")
    );
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
            .any(|omission| { omission["path"] == "README.md" && omission["reason"] == "non_source" })
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
fn java_and_c_sharp_map_is_first_class_and_preserves_visibility_duplicates_and_limitations() {
    let fixture = JavaCSharpMapFixtureRepository::new();
    let first = fixture.run(&["map", "--no-cache", "--json"]);
    let second = fixture.run(&["map", "--no-cache", "--json"]);
    let first_stdout = stdout(&first);
    let second_stdout = stdout(&second);
    let json: Value = serde_json::from_str(&first_stdout).expect("valid Java and C# map JSON");

    assert!(
        first.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "repeated map failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());
    assert_plain_report(&first_stdout);
    assert_eq!(
        first_stdout, second_stdout,
        "Java and C# map ordering must be deterministic"
    );
    assert_eq!(json["map"]["query_pack"], "mixed");
    assert_eq!(json["map"]["query_packs"]["java"], "java-v1");
    assert_eq!(json["map"]["query_packs"]["c_sharp"], "c-sharp-v1");

    let files = json["map"]["files"].as_array().expect("Java and C# map files");
    for (path, language, extension) in [
        ("src/service.java", "java", "java"),
        ("src/consumer.java", "java", "java"),
        ("src/service.cs", "c_sharp", "cs"),
    ] {
        let file = files
            .iter()
            .find(|file| file["path"] == path)
            .expect("first-class language fixture file");
        assert_eq!(file["language"], language);
        assert_eq!(file["extension"], extension);
        assert_eq!(file["status"], "complete");
        assert!(!file["symbols"].as_array().expect("symbols").is_empty());
    }
    let untracked = files
        .iter()
        .find(|file| file["path"] == "src/untracked.java")
        .expect("untracked Java file");
    assert_eq!(untracked["worktree_state"], "untracked");

    let java = files
        .iter()
        .find(|file| file["path"] == "src/service.java")
        .expect("Java file");
    assert!(
        java["symbols"].as_array().expect("Java symbols").iter().any(|symbol| {
            symbol["name"] == "example" && symbol["kind"] == "module" && symbol["role"] == "definition"
        })
    );
    assert!(
        java["symbols"].as_array().expect("Java symbols").iter().any(|symbol| {
            symbol["name"] == "Service" && symbol["kind"] == "class" && symbol["role"] == "definition"
        })
    );
    assert!(
        java["symbols"].as_array().expect("Java symbols").iter().any(|symbol| {
            symbol["name"] == "Hidden" && symbol["kind"] == "class" && symbol["role"] == "definition"
        })
    );
    assert!(java["symbols"].as_array().expect("Java symbols").iter().any(|symbol| {
        symbol["name"] == "run"
            && symbol["kind"] == "method"
            && symbol["role"] == "definition"
            && symbol["location"]["start"]["line"].as_u64().unwrap_or(0) > 0
            && symbol["context"]
                .as_str()
                .unwrap_or_default()
                .starts_with("public Result run")
    }));
    assert!(
        java["symbols"]
            .as_array()
            .expect("Java symbols")
            .iter()
            .any(|symbol| { symbol["name"] == "Input" && symbol["kind"] == "type" && symbol["role"] == "reference" })
    );

    let c_sharp = files
        .iter()
        .find(|file| file["path"] == "src/service.cs")
        .expect("C# file");
    assert!(c_sharp["symbols"].as_array().expect("C# symbols").iter().any(|symbol| {
        symbol["name"] == "Example.App" && symbol["kind"] == "module" && symbol["role"] == "definition"
    }));
    assert!(
        c_sharp["symbols"].as_array().expect("C# symbols").iter().any(|symbol| {
            symbol["name"] == "Service" && symbol["kind"] == "class" && symbol["role"] == "definition"
        })
    );
    assert!(
        c_sharp["symbols"].as_array().expect("C# symbols").iter().any(|symbol| {
            symbol["name"] == "Value" && symbol["kind"] == "struct" && symbol["role"] == "definition"
        })
    );
    assert!(
        c_sharp["symbols"].as_array().expect("C# symbols").iter().any(|symbol| {
            symbol["name"] == "Hidden" && symbol["kind"] == "class" && symbol["role"] == "definition"
        })
    );
    assert!(
        c_sharp["symbols"].as_array().expect("C# symbols").iter().any(|symbol| {
            symbol["name"] == "Execute" && symbol["kind"] == "method" && symbol["role"] == "reference"
        })
    );

    let broken = files
        .iter()
        .find(|file| file["path"] == "src/broken.cs")
        .expect("malformed C# file");
    assert_eq!(broken["status"], "partial");
    assert!(!broken["limitations"].as_array().expect("C# limitations").is_empty());

    let omissions = json["map"]["omissions"].as_array().expect("map omissions");
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "src/ignored.java" && omission["reason"] == "ignored_untracked" })
    );
    assert!(
        omissions
            .iter()
            .any(|omission| { omission["path"] == "README.md" && omission["reason"] == "non_source" })
    );
    assert!(
        !json["map"]["findings"]
            .as_array()
            .expect("map findings")
            .iter()
            .any(|finding| {
                finding["kind"] == "ambiguous_reference"
                    && finding["detail"].as_str().unwrap_or_default().contains("Service")
            })
    );

    let markdown = fixture.run(&["map"]);
    let markdown_stdout = stdout(&markdown);
    assert!(markdown.status.success());
    assert!(markdown.stderr.is_empty());
    assert!(markdown_stdout.contains("Java files"));
    assert!(markdown_stdout.contains("C# files"));
    assert!(markdown_stdout.contains("src/broken.cs"));
    assert!(markdown_stdout.contains("Tree-sitter reported parse errors in this C# file"));
    assert_plain_report(&markdown_stdout);
}

#[test]
fn format_json_and_json_alias_share_the_report_renderer() {
    let fixture = MixedMapFixtureRepository::new();
    let format_output = fixture.run(&["--no-cache", "--format", "json"]);
    let alias_output = fixture.run(&["--no-cache", "--json"]);

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
        "# Codeplat map\n\
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
         Inventory: 0 tracked (0 modified), 0 untracked, 0 analyzed, 0 omitted, 0 classified\n\
         \n\
         ### Rust files\n\
         \n\
         No Rust files were analyzed.\n\
         \n\
         ### Map limitations\n\
         \n\
         - Rust definitions and references are extracted lexically; only explicit same-file call evidence is graphed, and imports, types, macros, and runtime behavior are not semantically resolved.\n\
         - Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge.\n\
         - Tracked files are eligible even when ignore rules match them, except deterministic generated/vendor/minified classifications; exact focus paths can opt in within the safety limits.\n"
    );
}

#[test]
fn color_options_never_change_json_stdout() {
    let fixture = MixedMapFixtureRepository::new();
    let never = fixture.run(&["--no-cache", "--color", "never", "--json"]);
    let always = fixture.run(&["--no-cache", "--color", "always", "--json"]);

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
