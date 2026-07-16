use super::*;

pub fn rust_language() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

pub fn python_language() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

pub fn ruby_language() -> tree_sitter::Language {
    tree_sitter_ruby::LANGUAGE.into()
}

pub fn javascript_language() -> tree_sitter::Language {
    tree_sitter_javascript::LANGUAGE.into()
}

pub fn typescript_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

pub fn tsx_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

pub fn java_language() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

pub fn c_sharp_language() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

pub fn go_language() -> tree_sitter::Language {
    tree_sitter_go::LANGUAGE.into()
}

pub fn support_for_path(path: &Path) -> Option<&'static LanguageSupport> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    LANGUAGE_SUPPORT
        .iter()
        .find(|support| support.extensions.contains(&extension.as_str()))
}

pub fn is_source_like_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "c" | "cc"
            | "cpp"
            | "cxx"
            | "h"
            | "hh"
            | "hpp"
            | "hxx"
            | "go"
            | "fs"
            | "fsi"
            | "fsx"
            | "ex"
            | "exs"
            | "erl"
            | "hrl"
            | "lua"
            | "php"
            | "swift"
            | "kt"
            | "kts"
            | "scala"
            | "sh"
            | "bash"
            | "zsh"
            | "nu"
            | "zig"
            | "vue"
            | "svelte"
            | "dart"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "sql"
    ) {
        return true;
    }
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("Dockerfile" | "Justfile" | "Makefile" | "Rakefile")
    )
}

pub fn extension_for_path(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(String::new, |extension| extension.to_ascii_lowercase())
}

pub fn supported_query_packs(files: &[SourceFile]) -> BTreeMap<String, String> {
    let mut query_packs = BTreeMap::new();
    for file in files {
        if let Some(support) = support_for_path(Path::new(&file.path)) {
            query_packs.insert(support.language.label().to_owned(), support.query_pack.to_owned());
        }
    }
    if query_packs.is_empty() {
        query_packs.insert(
            RUST_SUPPORT.language.label().to_owned(),
            RUST_SUPPORT.query_pack.to_owned(),
        );
    }
    query_packs
}
