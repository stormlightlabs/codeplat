use super::*;

pub fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

pub fn source_fingerprint(source: &[u8]) -> String {
    digest_hex(source)
}

pub fn query_digest(support: &LanguageSupport) -> String {
    let mut identity = Vec::new();
    identity.extend_from_slice(support.query_pack.as_bytes());
    identity.push(0);
    identity.extend_from_slice(support.definitions.as_bytes());
    identity.push(0);
    identity.extend_from_slice(support.references.as_bytes());
    digest_hex(&identity)
}

pub fn unix_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

pub fn cache_record_mtime(path: &Path) -> std::time::SystemTime {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

pub fn normalized_cache_file_path(requested: &str, repository_root: &Path) -> Option<String> {
    let requested = requested.trim().replace('\\', "/");
    if requested.is_empty() {
        return None;
    }
    let requested_path = Path::new(&requested);
    let relative = if requested_path.is_absolute() {
        requested_path.strip_prefix(repository_root).ok()?
    } else {
        requested_path
    };
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(component) => components.push(component.to_str()?.to_owned()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop()?;
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return None,
        }
    }
    (!components.is_empty()).then(|| components.join("/"))
}

pub fn cache_status(mode: CacheMode, stats: &CacheStats) -> CacheStatus {
    if mode == CacheMode::Disabled {
        CacheStatus::Disabled
    } else if !stats.stale.is_empty() {
        CacheStatus::Stale
    } else if !stats.refreshed.is_empty() {
        CacheStatus::Refreshed
    } else if stats.hits > 0 {
        CacheStatus::Hit
    } else {
        CacheStatus::Miss
    }
}

pub fn absolute_path(path: &Path) -> Result<PathBuf> {
    security::absolute_input_path(path).map_err(|error| MapError::analysis("reading the current directory", error))
}

pub fn repository_head_snapshot(repository: &gix::Repository) -> Result<HeadSnapshot> {
    let reference = repository
        .head_name()
        .map_err(|error| MapError::analysis("resolving HEAD reference", error))?
        .map(|name| name.as_bstr().to_str_lossy().into_owned());
    let oid = repository.head_id().ok().map(|id| id.to_string());
    Ok(HeadSnapshot {
        detached: reference.is_none() && oid.is_some(),
        unborn: reference.is_some() && oid.is_none(),
        reference,
        oid,
    })
}

pub fn worktree_snapshot(inventory: (usize, usize, usize)) -> WorktreeSnapshot {
    let (tracked_files, modified_files, untracked_files) = inventory;
    let state = match (modified_files > 0, untracked_files > 0) {
        (false, false) => WorktreeSnapshotState::Clean,
        (true, false) => WorktreeSnapshotState::Modified,
        (false, true) => WorktreeSnapshotState::Untracked,
        (true, true) => WorktreeSnapshotState::Mixed,
    };
    WorktreeSnapshot { state, observed: true, tracked_files, modified_files, untracked_files, detail: None }
}

pub fn build_exclusions(root: &Path, exclusions: &[String]) -> Result<Option<Gitignore>> {
    if exclusions.is_empty() {
        return Ok(None);
    }
    let mut builder = GitignoreBuilder::new(root);
    for exclusion in exclusions {
        if exclusion.trim().is_empty() {
            return Err(MapError::Input {
                path: root.to_owned(),
                reason: "exclusion globs must not be empty".to_owned(),
            });
        }
        builder.add_line(None, exclusion).map_err(|error| MapError::Input {
            path: root.to_owned(),
            reason: format!("invalid exclusion glob `{exclusion}`: {error}"),
        })?;
    }
    builder.build().map(Some).map_err(|error| MapError::Input {
        path: root.to_owned(),
        reason: format!("could not compile exclusion globs: {error}"),
    })
}

pub fn explicitly_excluded(exclusions: Option<&Gitignore>, path: &Path) -> bool {
    exclusions.is_some_and(|matcher| matcher.matched_path_or_any_parents(path, false).is_ignore())
}

pub fn collect_tree_files(
    repository: &gix::Repository, tree_id: &gix::Id<'_>, prefix: &[u8], files: &mut BTreeSet<String>,
    submodule_paths: &mut BTreeSet<String>, max_files: usize, max_depth: usize,
) -> Result<()> {
    let mut stack = vec![(tree_id.detach(), prefix.to_vec(), 0usize)];
    while let Some((tree_id, prefix, depth)) = stack.pop() {
        if files.len() >= max_files || depth > max_depth {
            break;
        }
        let tree = repository
            .find_tree(tree_id)
            .map_err(|error| MapError::analysis("reading the tracked source tree", error))?;
        let mut names = BTreeSet::new();
        let mut entries = Vec::new();
        for entry in tree.iter() {
            if entries.len() >= max_files {
                break;
            }
            let entry = entry.map_err(|error| MapError::analysis("decoding a tracked tree entry", error))?;
            let name = entry.filename().as_bytes().to_owned();
            if !names.insert(name) {
                return Err(MapError::safety(
                    "decoding a tracked tree path",
                    security::PathSafetyError { kind: security::PathSafetyKind::Collision },
                ));
            }
            entries.push(entry);
        }
        entries.reverse();
        for entry in entries {
            if files.len() >= max_files {
                break;
            }
            let mut path_bytes = prefix.clone();
            if !path_bytes.is_empty() {
                path_bytes.push(b'/');
            }
            path_bytes.extend_from_slice(entry.filename().as_bytes());
            let path = security::validate_repository_path(&path_bytes)
                .map_err(|error| MapError::safety("decoding a tracked tree path", error))?;
            if entry.mode().is_tree() {
                stack.push((entry.id().detach(), path.into_bytes(), depth.saturating_add(1)));
            } else {
                let is_submodule = entry.mode().is_commit();
                if !files.insert(path.clone()) {
                    return Err(MapError::safety(
                        "decoding a tracked tree path",
                        security::PathSafetyError { kind: security::PathSafetyKind::Collision },
                    ));
                }
                if is_submodule {
                    submodule_paths.insert(path);
                }
            }
        }
    }
    Ok(())
}

pub fn collect_index_files(repository: &gix::Repository, files: &mut BTreeSet<String>, max_files: usize) -> Result<()> {
    let index = repository
        .index_or_empty()
        .map_err(|error| MapError::analysis("reading the worktree index", error))?;
    for (path, _) in index.entries_with_paths_by_filter_map(|_, entry| Some(entry.id)) {
        if files.len() >= max_files {
            break;
        }
        let path = security::validate_repository_path(path.as_bytes())
            .map_err(|error| MapError::safety("decoding a worktree index path", error))?;
        files.insert(path);
    }
    Ok(())
}

pub fn collect_modified_paths(
    repository: &gix::Repository, repository_root: &Path, max_file_bytes: usize,
) -> Result<BTreeSet<String>> {
    let index = repository
        .index_or_load_from_head_or_empty()
        .map_err(|error| MapError::analysis("loading the worktree index for status", error))?;
    let mut modified = BTreeSet::new();
    for (path, id) in index.entries_with_paths_by_filter_map(|_, entry| Some(entry.id)) {
        let path = security::validate_repository_path(path.as_bytes())
            .map_err(|error| MapError::safety("decoding a status path", error))?;
        let worktree =
            match security::read_worktree_file_limited(repository_root, repository_root, &path, max_file_bytes) {
                Ok(bytes) => bytes,
                Err(security::ReadError::Safety(_)) => {
                    modified.insert(path);
                    continue;
                }
                Err(security::ReadError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    modified.insert(path);
                    continue;
                }
                Err(security::ReadError::Io(_)) => {
                    modified.insert(path);
                    continue;
                }
            };
        let blob = repository
            .find_blob(id)
            .map_err(|error| MapError::analysis("reading an index blob for status", error))?;
        if blob.data != worktree {
            modified.insert(path);
        }
    }
    Ok(modified)
}

pub fn walk_files(
    root: &Path, repository_root: &Path, standard_filters: bool, max_entries: usize, recursive: bool,
) -> (BTreeMap<String, bool>, Vec<WalkIssue>) {
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(standard_filters)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    let filter_repository_root = repository_root.to_owned();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 || !entry.file_type().is_some_and(|file_type| file_type.is_dir()) {
            return true;
        }
        !pruned_directory(entry.path(), &filter_repository_root, recursive)
    });
    let mut files = BTreeMap::new();
    let mut errors = Vec::new();
    let mut native_paths = BTreeMap::<String, PathBuf>::new();
    for item in builder.build() {
        if files.len() >= max_entries {
            errors.push(WalkIssue::Traversal(format!(
                "file inventory reached the {}-path limit; deeper paths were not visited",
                max_entries
            )));
            break;
        }
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(WalkIssue::Traversal(format!(
                    "ignore traversal reported an error: {error}"
                )));
                continue;
            }
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !(file_type.is_file() || entry.path_is_symlink()) {
            continue;
        }
        let Some(path) = entry.path().strip_prefix(repository_root).ok() else {
            continue;
        };
        let path = match security::validate_os_relative_path(path) {
            Ok(path) => path,
            Err(error) => {
                errors.push(WalkIssue::Safety(format!("walked path rejected: {error}")));
                continue;
            }
        };
        if native_paths.get(&path).is_some_and(|previous| previous != entry.path()) {
            errors.push(WalkIssue::Safety(
                "two filesystem paths collapsed to one validated repository path".to_owned(),
            ));
            continue;
        }
        native_paths.insert(path.clone(), entry.path().to_owned());
        files.insert(path, entry.path_is_symlink());
    }
    (files, errors)
}

pub fn pruned_directory(path: &Path, repository_root: &Path, recursive: bool) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if matches!(
        name,
        ".git" | "target" | "node_modules" | "vendor" | "dist" | "build" | "out" | "coverage"
    ) {
        return true;
    }
    (!recursive)
        && path != repository_root
        && fs::symlink_metadata(path.join(".git")).is_ok_and(|metadata| metadata.is_dir() || metadata.is_file())
}

pub fn in_scope(path: &str, scope: &str) -> bool {
    scope == "." || path == scope || path.starts_with(&format!("{scope}/"))
}

pub fn is_git_internal(path: &str) -> bool {
    path == ".git" || path.starts_with(".git/")
}

pub fn inventory(candidates: &BTreeMap<String, Candidate>) -> (usize, usize, usize) {
    let tracked = candidates
        .values()
        .filter(|candidate| !matches!(candidate.state, WorktreeState::Untracked))
        .count();
    let modified = candidates
        .values()
        .filter(|candidate| matches!(candidate.state, WorktreeState::Modified))
        .count();
    let untracked = candidates
        .values()
        .filter(|candidate| matches!(candidate.state, WorktreeState::Untracked))
        .count();
    (tracked, modified, untracked)
}
