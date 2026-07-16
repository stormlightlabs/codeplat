#[cfg(unix)]
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static CACHE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathSafetyKind {
    Empty,
    Nul,
    Absolute,
    Parent,
    Current,
    EmptyComponent,
    PlatformSeparator,
    Prefix,
    NonUtf8,
    Symlink,
    ScopeEscape,
    CacheInsideRepository,
    CacheSymlink,
    Collision,
}

impl PathSafetyKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Nul => "nul",
            Self::Absolute => "absolute",
            Self::Parent => "parent",
            Self::Current => "current",
            Self::EmptyComponent => "empty_component",
            Self::PlatformSeparator => "platform_separator",
            Self::Prefix => "prefix",
            Self::NonUtf8 => "non_utf8",
            Self::Symlink => "symlink",
            Self::ScopeEscape => "scope_escape",
            Self::CacheInsideRepository => "cache_inside_repository",
            Self::CacheSymlink => "cache_symlink",
            Self::Collision => "collision",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unsafe repository path ({kind})")]
pub struct PathSafetyError {
    pub kind: PathSafetyKind,
}

impl PathSafetyError {
    fn new(kind: PathSafetyKind) -> Self {
        Self { kind }
    }
}

impl std::fmt::Display for PathSafetyKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("scope input is invalid: {0}")]
    Input(String),
    #[error(transparent)]
    Safety(#[from] PathSafetyError),
}

#[derive(Debug, thiserror::Error)]
pub enum CacheRootError {
    #[error("the cache root resolves inside the analyzed repository")]
    InsideRepository,
    #[error("could not resolve the cache root: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CacheWriteError {
    #[error(transparent)]
    Safety(#[from] PathSafetyError),
    #[error("cache write failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Safety(#[from] PathSafetyError),
    #[error("worktree read failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub struct RepositoryScope {
    pub repository_root: PathBuf,
    pub selected_path: PathBuf,
    pub relative_path: String,
}

/// Validate a single Git tree/index path component before it is converted to an OS path.
pub fn validate_component(bytes: &[u8]) -> Result<String, PathSafetyError> {
    if bytes.is_empty() {
        return Err(PathSafetyError::new(PathSafetyKind::EmptyComponent));
    }
    if bytes.contains(&0) {
        return Err(PathSafetyError::new(PathSafetyKind::Nul));
    }
    if bytes == b"." {
        return Err(PathSafetyError::new(PathSafetyKind::Current));
    }
    if bytes == b".." {
        return Err(PathSafetyError::new(PathSafetyKind::Parent));
    }
    if bytes.contains(&b'/') || bytes.contains(&b'\\') {
        return Err(PathSafetyError::new(PathSafetyKind::PlatformSeparator));
    }
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(PathSafetyError::new(PathSafetyKind::Prefix));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| PathSafetyError::new(PathSafetyKind::NonUtf8))?;
    let path = Path::new(text);
    if path.is_absolute() {
        return Err(PathSafetyError::new(PathSafetyKind::Absolute));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir | Component::CurDir
        )
    }) {
        return Err(PathSafetyError::new(PathSafetyKind::Prefix));
    }
    Ok(text.to_owned())
}

/// Validate a slash-separated Git path and only then return its UTF-8 representation.
pub fn validate_repository_path(bytes: &[u8]) -> Result<String, PathSafetyError> {
    if bytes.is_empty() {
        return Err(PathSafetyError::new(PathSafetyKind::Empty));
    }
    if bytes.starts_with(b"/") || bytes.starts_with(b"\\") {
        return Err(PathSafetyError::new(PathSafetyKind::Absolute));
    }
    let mut components = Vec::new();
    for component in bytes.split(|byte| *byte == b'/') {
        components.push(validate_component(component)?);
    }
    if components.is_empty() {
        return Err(PathSafetyError::new(PathSafetyKind::Empty));
    }
    Ok(components.join("/"))
}

/// Convert a native filesystem-relative path into the slash-separated internal path format.
pub fn validate_os_relative_path(path: &Path) -> Result<String, PathSafetyError> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => {
                #[cfg(unix)]
                let bytes = os_str_bytes(component);
                #[cfg(not(unix))]
                let bytes = component
                    .to_str()
                    .ok_or_else(|| PathSafetyError::new(PathSafetyKind::NonUtf8))?
                    .as_bytes();
                components.push(validate_component(bytes)?);
            }
            Component::Prefix(_) => return Err(PathSafetyError::new(PathSafetyKind::Prefix)),
            Component::RootDir => return Err(PathSafetyError::new(PathSafetyKind::Absolute)),
            Component::ParentDir => return Err(PathSafetyError::new(PathSafetyKind::Parent)),
            Component::CurDir => return Err(PathSafetyError::new(PathSafetyKind::Current)),
        }
    }
    if components.is_empty() {
        return Err(PathSafetyError::new(PathSafetyKind::Empty));
    }
    Ok(components.join("/"))
}

pub fn absolute_input_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() { Ok(path.to_owned()) } else { Ok(std::env::current_dir()?.join(path)) }
}

pub fn discover_repository(directory: &Path) -> Result<gix::Repository, Box<gix::discover::Error>> {
    let current_dir = std::env::current_dir().ok();
    let options = gix::discover::upwards::Options {
        trust: gix::discover::upwards::TrustPolicy::Assume(gix::sec::Trust::Reduced),
        current_dir: current_dir.as_deref(),
        ..Default::default()
    };
    let open_options = restrictive_open_options();
    let trust_map = gix::sec::trust::Mapping { full: open_options.clone(), reduced: open_options };
    gix::ThreadSafeRepository::discover_opts(directory, options, trust_map)
        .map(Into::into)
        .map_err(Box::new)
}

pub fn resolve_scope(repository: &gix::Repository, selected_path: &Path) -> Result<RepositoryScope, ScopeError> {
    let selected_input = selected_path.to_owned();
    let raw_root = repository
        .workdir()
        .ok_or_else(|| ScopeError::Input("the discovered repository has no worktree".to_owned()))?;
    let repository_root = fs::canonicalize(raw_root).map_err(|error| ScopeError::Input(error.to_string()))?;
    let selected_path = fs::canonicalize(&selected_input).map_err(|error| ScopeError::Input(error.to_string()))?;
    if !selected_path.is_dir() {
        return Err(ScopeError::Input("the selected scope is not a directory".to_owned()));
    }
    let relative = selected_path
        .strip_prefix(&repository_root)
        .map_err(|_| ScopeError::Input(format!("path is outside repository `{}`", repository_root.display())))?;
    let relative_path =
        if relative.as_os_str().is_empty() { ".".to_owned() } else { validate_os_relative_path(relative)? };

    if let Ok(raw_relative) = selected_input.strip_prefix(raw_root) {
        reject_symlink_components(raw_root, raw_relative)?;
    } else if fs::symlink_metadata(&selected_input).is_ok_and(|metadata| is_reparse_or_symlink(&metadata)) {
        return Err(ScopeError::Safety(PathSafetyError::new(PathSafetyKind::Symlink)));
    }
    if repository_root.to_str().is_none() || selected_path.to_str().is_none() {
        return Err(ScopeError::Safety(PathSafetyError::new(PathSafetyKind::NonUtf8)));
    }
    Ok(RepositoryScope { repository_root, selected_path, relative_path })
}
fn restrictive_open_options() -> gix::open::Options {
    gix::open::Options::isolated()
        .strict_config(true)
        .filter_config_section(reject_repository_config)
        .config_overrides([
            "core.bare=false",
            "status.showUntrackedFiles=none",
            "diff.renames=false",
            "submodule.recurse=false",
        ])
}

pub fn cache_root(repository_root: &Path) -> Result<Option<PathBuf>, CacheRootError> {
    match cache_base_path() {
        Some(configured) => {
            let resolved = resolve_existing(&configured)?;
            if is_within(repository_root, &configured) || is_within(repository_root, &resolved) {
                Err(CacheRootError::InsideRepository)
            } else {
                Ok(Some(resolved))
            }
        }
        None => Ok(None),
    }
}

/// Resolve the configured Codeplat cache root without consulting a repository.
/// Cache-management commands use this so they never discover or modify a target worktree.
pub fn configured_cache_root() -> Result<Option<PathBuf>, CacheRootError> {
    let Some(candidate) = cache_base_path() else {
        return Ok(None);
    };
    Ok(Some(resolve_existing(&candidate)?))
}

pub fn write_cache_file(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), CacheWriteError> {
    fs::create_dir_all(root)?;
    set_private_directory(root)?;
    cache_path_is_safe(root, path).map_err(CacheWriteError::Safety)?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| is_reparse_or_symlink(&metadata)) {
        return Err(CacheWriteError::Safety(PathSafetyError::new(
            PathSafetyKind::CacheSymlink,
        )));
    }
    #[cfg(unix)]
    write_cache_beneath(root, path, bytes)?;
    #[cfg(not(unix))]
    {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cache record has no parent"))?;
        fs::create_dir_all(parent)?;
        cache_path_is_safe(root, path).map_err(CacheWriteError::Safety)?;
        let temporary = parent.join(format!(
            ".{}.tmp-{}-{}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("record"),
            std::process::id(),
            CACHE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        write_file_no_follow(&temporary, bytes)?;
        match fs::rename(&temporary, path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // Windows does not replace an existing file with rename. The
                // temporary file is complete, so the fallback still avoids
                // exposing truncated JSON; Unix uses the atomic rename path.
                fs::remove_file(path)?;
                fs::rename(&temporary, path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn cache_path_is_safe(root: &Path, path: &Path) -> Result<(), PathSafetyError> {
    if !is_within(root, path) {
        return Err(PathSafetyError::new(PathSafetyKind::CacheInsideRepository));
    }
    let relative = path.strip_prefix(root).expect("path was checked to be beneath root");
    let mut current = root.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(PathSafetyError::new(PathSafetyKind::Prefix));
        };
        current.push(component);
        if fs::symlink_metadata(&current).is_ok_and(|metadata| is_reparse_or_symlink(&metadata)) {
            return Err(PathSafetyError::new(PathSafetyKind::CacheSymlink));
        }
    }
    Ok(())
}

pub fn read_cache_file(root: &Path, path: &Path) -> Result<Vec<u8>, ReadError> {
    const MAX_CACHE_RECORD_BYTES: usize = 32 * 1024 * 1024;
    cache_path_is_safe(root, path).map_err(ReadError::Safety)?;
    #[cfg(unix)]
    {
        let relative = path
            .strip_prefix(root)
            .expect("cache path was checked to be beneath the cache root");
        let relative = validate_os_relative_path(relative)?;
        read_beneath_limited(root, &relative, MAX_CACHE_RECORD_BYTES)
    }
    #[cfg(not(unix))]
    {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_CACHE_RECORD_BYTES.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_CACHE_RECORD_BYTES {
            return Err(ReadError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "cache record exceeds the 32 MiB read limit",
            )));
        }
        Ok(bytes)
    }
}

/// Read a worktree file without following path components and without
/// allocating more than `max_bytes` plus one byte to detect oversize input.
pub fn read_worktree_file_limited(
    repository_root: &Path, scope_root: &Path, relative_path: &str, max_bytes: usize,
) -> Result<Vec<u8>, ReadError> {
    let validated = validate_repository_path(relative_path.as_bytes())?;
    let candidate = repository_root.join(&validated);
    if !is_within(repository_root, &candidate) || !is_within(scope_root, &candidate) {
        return Err(ReadError::Safety(PathSafetyError::new(PathSafetyKind::ScopeEscape)));
    }
    preflight_no_follow(repository_root, &validated)?;

    #[cfg(unix)]
    {
        read_beneath_limited(repository_root, &validated, max_bytes)
    }
    #[cfg(not(unix))]
    {
        let mut file = File::open(candidate)?;
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(ReadError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("file exceeds the {max_bytes}-byte analysis limit"),
            )));
        }
        Ok(bytes)
    }
}

fn cache_base_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".cache"))
        });
    let base = base?;
    if !base.is_absolute() {
        return None;
    }
    Some(base.join("codeplat"))
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<(), io::Error> {
    Ok(())
}

fn reject_repository_config(meta: &gix::config::file::Metadata) -> bool {
    meta.source.kind() != gix::config::source::Kind::Repository
}

fn reject_symlink_components(root: &Path, relative: &Path) -> Result<(), ScopeError> {
    let mut current = root.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(ScopeError::Safety(PathSafetyError::new(PathSafetyKind::Prefix)));
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| ScopeError::Input(error.to_string()))?;
        if is_reparse_or_symlink(&metadata) {
            return Err(ScopeError::Safety(PathSafetyError::new(PathSafetyKind::Symlink)));
        }
    }
    Ok(())
}

fn resolve_existing(path: &Path) -> Result<PathBuf, io::Error> {
    let mut existing = path.to_owned();
    let mut suffix = Vec::new();
    while fs::symlink_metadata(&existing).is_err() {
        let Some(name) = existing.file_name() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "cache root has no existing ancestor",
            ));
        };
        suffix.push(name.to_owned());
        existing.pop();
    }
    let mut resolved = fs::canonicalize(existing)?;
    for component in suffix.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn preflight_no_follow(root: &Path, relative: &str) -> Result<(), ReadError> {
    let mut current = root.to_owned();
    let components = relative.split('/').collect::<Vec<_>>();
    for component in components {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)?;
        if is_reparse_or_symlink(&metadata) {
            return Err(ReadError::Safety(PathSafetyError::new(PathSafetyKind::Symlink)));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn read_beneath_limited(root: &Path, relative: &str, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;

    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .map_err(map_read_open_error)?;
    let components = relative.split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let name = std::ffi::CString::new(component.as_bytes()).expect("validated paths contain no NUL");
        let flags = if index + 1 == components.len() {
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ELOOP) {
                return Err(ReadError::Safety(PathSafetyError::new(PathSafetyKind::Symlink)));
            }
            return Err(ReadError::Io(error));
        }
        if index + 1 == components.len() {
            let file = unsafe { File::from_raw_fd(fd) };
            let mut bytes = Vec::new();
            file.take(max_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
            if bytes.len() > max_bytes {
                return Err(ReadError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("file exceeds the {max_bytes}-byte analysis limit"),
                )));
            }
            return Ok(bytes);
        }
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Err(ReadError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        "empty repository path",
    )))
}

#[cfg(unix)]
fn write_cache_beneath(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), CacheWriteError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;

    let relative = path
        .strip_prefix(root)
        .expect("cache path was checked to be beneath the cache root");
    let relative = validate_os_relative_path(relative)?;
    let components = relative.split('/').collect::<Vec<_>>();
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .map_err(map_cache_open_error)?;
    if unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } < 0 {
        return Err(io::Error::last_os_error().into());
    }

    for component in &components[..components.len().saturating_sub(1)] {
        let name = std::ffi::CString::new(component.as_bytes()).expect("validated paths contain no NUL");
        let mut fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 && io::Error::last_os_error().kind() == io::ErrorKind::NotFound {
            let mkdir_result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if mkdir_result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error.into());
                }
            }
            fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
        }
        if fd < 0 {
            return Err(map_cache_open_error(io::Error::last_os_error()));
        }
        let child = unsafe { File::from_raw_fd(fd) };
        if unsafe { libc::fchmod(child.as_raw_fd(), 0o700) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        directory = child;
    }

    let Some(filename) = components.last() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cache record has no filename").into());
    };
    let final_name = std::ffi::CString::new(filename.as_bytes()).expect("validated paths contain no NUL");
    let temporary_name = format!(
        ".{filename}.tmp-{}-{}",
        std::process::id(),
        CACHE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temporary_name = std::ffi::CString::new(temporary_name).expect("temporary names contain no NUL");
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            temporary_name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(map_cache_open_error(io::Error::last_os_error()));
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let result = (|| -> Result<(), CacheWriteError> {
        file.write_all(bytes)?;
        file.sync_all()?;
        if unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                temporary_name.as_ptr(),
                directory.as_raw_fd(),
                final_name.as_ptr(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error().into());
        }
        if unsafe { libc::fsync(directory.as_raw_fd()) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    })();
    if result.is_err() {
        unsafe {
            libc::unlinkat(directory.as_raw_fd(), temporary_name.as_ptr(), 0);
        }
    }
    result
}

#[cfg(unix)]
fn map_read_open_error(error: io::Error) -> ReadError {
    if error.raw_os_error() == Some(libc::ELOOP) {
        ReadError::Safety(PathSafetyError::new(PathSafetyKind::Symlink))
    } else {
        ReadError::Io(error)
    }
}

#[cfg(unix)]
fn map_cache_open_error(error: io::Error) -> CacheWriteError {
    if error.raw_os_error() == Some(libc::ELOOP) {
        CacheWriteError::Safety(PathSafetyError::new(PathSafetyKind::CacheSymlink))
    } else {
        CacheWriteError::Io(error)
    }
}

#[cfg(not(unix))]
fn write_file_no_follow(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn is_within(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::{FileTypeExt, MetadataExt};
        return metadata.file_type().is_symlink()
            || metadata.file_attributes() & 0x400 != 0
            || metadata.file_type().is_symlink_dir()
            || metadata.file_type().is_symlink_file();
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(unix)]
fn os_str_bytes(value: &OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn rejects_escape_and_platform_paths_before_joining() {
        for path in [b"../outside".as_slice(), b"/outside", b"a\\b", b"C:/outside", b"a//b"] {
            assert!(
                validate_repository_path(path).is_err(),
                "path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn rejects_non_utf8_paths_instead_of_lossy_collisions() {
        assert_eq!(
            validate_repository_path(b"bad\xff")
                .expect_err("non-UTF-8 path must be rejected")
                .kind,
            PathSafetyKind::NonUtf8
        );
    }

    #[test]
    fn preserves_case_distinct_paths_without_merging_them() {
        let paths: BTreeSet<_> = [b"src/Readme.rs".as_slice(), b"src/README.rs".as_slice()]
            .into_iter()
            .map(|path| validate_repository_path(path).expect("valid path"))
            .collect();
        assert_eq!(paths.len(), 2);
    }
}
