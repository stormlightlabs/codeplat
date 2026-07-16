use super::*;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRootKind {
    Workspace,
    Package,
    Mixed,
    #[default]
    Unknown,
}

impl ProjectRootKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Package => "package",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LandmarkKind {
    Readme,
    ContributorInstructions,
    AgentInstructions,
    Manifest,
    Lockfile,
    WorkspaceRoot,
    PackageRoot,
    BuildEntryPoint,
    TaskEntryPoint,
    TestRoot,
    Ci,
    Ownership,
    License,
    Submodule,
    NestedRepository,
    Unknown,
}

impl LandmarkKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Readme => "readme",
            Self::ContributorInstructions => "contributor_instructions",
            Self::AgentInstructions => "agent_instructions",
            Self::Manifest => "manifest",
            Self::Lockfile => "lockfile",
            Self::WorkspaceRoot => "workspace_root",
            Self::PackageRoot => "package_root",
            Self::BuildEntryPoint => "build_entry_point",
            Self::TaskEntryPoint => "task_entry_point",
            Self::TestRoot => "test_root",
            Self::Ci => "ci",
            Self::Ownership => "ownership",
            Self::License => "license",
            Self::Submodule => "submodule",
            Self::NestedRepository => "nested_repository",
            Self::Unknown => "unknown",
        }
    }

    pub const fn priority(self) -> u8 {
        match self {
            Self::AgentInstructions => 100,
            Self::Readme => 95,
            Self::ContributorInstructions => 90,
            Self::Manifest | Self::WorkspaceRoot | Self::PackageRoot => 85,
            Self::Lockfile => 80,
            Self::BuildEntryPoint | Self::TaskEntryPoint => 75,
            Self::TestRoot => 70,
            Self::Ci => 65,
            Self::Ownership => 60,
            Self::License => 50,
            Self::Submodule | Self::NestedRepository => 45,
            Self::Unknown => 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapReport {
    #[serde(default)]
    pub profile: AnalysisProfile,
    pub repository_root: String,
    pub scope_path: String,
    #[serde(default)]
    pub head: HeadSnapshot,
    #[serde(default)]
    pub worktree: WorktreeSnapshot,
    pub query_pack: String,
    /// Query-pack provenance for every language variant encountered in this map.
    #[serde(default)]
    pub query_packs: BTreeMap<String, String>,
    pub exclusions: Vec<String>,
    pub inventory: MapInventory,
    #[serde(default)]
    pub classifications: MapClassificationSummary,
    pub files: Vec<SourceFile>,
    pub omissions: Vec<SourceOmission>,
    pub findings: Vec<MapFinding>,
    pub limitations: Vec<String>,
    pub edges: Vec<LexicalEdge>,
    pub ranking: Vec<FileRank>,
    pub selection: MapSelection,
    pub cache: MapCacheReport,
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
    #[serde(default)]
    pub project_roots: Vec<ProjectRoot>,
    #[serde(default)]
    pub collections: MapCollections,
    /// Complete quality counts retained even when compact evidence samples are truncated.
    #[serde(default)]
    pub availability: MapAvailability,
    #[serde(skip)]
    pub reading_evidence: ReadingPlanEvidence,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapAvailability {
    pub unsupported_paths: usize,
    pub partial_files: usize,
    #[serde(default)]
    pub cache_unavailable_paths: usize,
    #[serde(default)]
    pub resource_limited: bool,
    #[serde(default)]
    pub unsafe_paths: usize,
    #[serde(default)]
    pub unsupported_path_names: Vec<String>,
    #[serde(default)]
    pub partial_path_names: Vec<String>,
    #[serde(default)]
    pub cache_unavailable_path_names: Vec<String>,
}

impl MapReport {
    pub fn compact_payload_tokens(&self) -> usize {
        let mut tokens = 0usize;
        for file in &self.files {
            tokens = tokens.saturating_add(token_count(&file.path));
            tokens = tokens
                .saturating_add(token_count(file.language.label()))
                .saturating_add(token_count(&file.extension))
                .saturating_add(token_count(file.worktree_state.label()))
                .saturating_add(token_count(file.status.label()));
            tokens = tokens.saturating_add(
                file.limitations
                    .iter()
                    .map(|limitation| token_count(limitation))
                    .sum::<usize>(),
            );
            tokens = tokens.saturating_add(
                file.classifications
                    .iter()
                    .map(|classification| {
                        token_count(classification.kind.label()) + token_count(&classification.reason)
                    })
                    .sum::<usize>(),
            );
            for symbol in &file.symbols {
                tokens = tokens.saturating_add(token_count(&symbol.name));
                tokens = tokens.saturating_add(token_count(&symbol.context));
                tokens = tokens.saturating_add(token_count(symbol.kind.label()));
                tokens = tokens.saturating_add(token_count(symbol.role.label()));
                tokens = tokens.saturating_add(token_count(symbol.visibility.label()));
                tokens = tokens.saturating_add(token_count(symbol.evidence.label()));
                tokens = tokens.saturating_add(symbol.scope.iter().map(|scope| token_count(scope)).sum::<usize>());
                tokens = tokens.saturating_add(8);
            }
        }
        for omission in &self.omissions {
            tokens = tokens.saturating_add(token_count(&omission.path));
            tokens = tokens.saturating_add(token_count(&omission.detail));
            tokens = tokens.saturating_add(
                omission
                    .classifications
                    .iter()
                    .map(|classification| {
                        token_count(classification.kind.label()) + token_count(&classification.reason)
                    })
                    .sum::<usize>(),
            );
        }
        for finding in &self.findings {
            tokens = tokens.saturating_add(token_count(finding.kind.label()));
            tokens = tokens.saturating_add(token_count(&finding.path));
            tokens = tokens.saturating_add(token_count(&finding.detail));
            tokens = tokens.saturating_add(if finding.location.is_some() { 8 } else { 0 });
        }
        for edge in &self.edges {
            tokens = tokens.saturating_add(token_count(&edge.source));
            tokens = tokens.saturating_add(token_count(&edge.target));
            tokens = tokens.saturating_add(token_count(&edge.symbol));
            tokens = tokens.saturating_add(
                edge.candidates
                    .iter()
                    .map(|candidate| token_count(candidate))
                    .sum::<usize>(),
            );
            tokens = tokens
                .saturating_add(token_count(&edge.candidate_group))
                .saturating_add(token_count(edge.resolution_reason.label()))
                .saturating_add(token_count(edge.confidence.label()))
                .saturating_add(token_count(edge.target_visibility.label()));
            tokens = tokens.saturating_add(if edge.ambiguous { 1 } else { 0 });
        }
        tokens = tokens.saturating_add(
            self.ranking
                .iter()
                .map(|rank| token_count(&rank.path).saturating_add(2))
                .sum::<usize>(),
        );
        let landmark_tokens = self
            .landmarks
            .iter()
            .map(|landmark| {
                token_count(landmark.kind.label())
                    + token_count(&landmark.path)
                    + token_count(&landmark.reason)
                    + token_count(landmark.worktree_state.label())
                    + landmark.project_root.as_deref().map_or(0, token_count)
                    + 2
            })
            .sum::<usize>();
        let project_root_tokens = self
            .project_roots
            .iter()
            .map(|root| {
                token_count(&root.path)
                    + token_count(root.kind.label())
                    + root
                        .manifests
                        .iter()
                        .map(|manifest| token_count(manifest))
                        .sum::<usize>()
                    + root
                        .recommended_paths
                        .iter()
                        .map(|path| token_count(path))
                        .sum::<usize>()
                    + 4
            })
            .sum::<usize>();
        tokens = tokens
            .saturating_add(landmark_tokens)
            .saturating_add(project_root_tokens);
        tokens
            .saturating_add(
                self.selection
                    .snippets
                    .iter()
                    .map(|snippet| snippet.estimated_tokens)
                    .sum::<usize>(),
            )
            .saturating_add(
                self.classifications
                    .samples
                    .iter()
                    .map(|sample| {
                        token_count(&sample.path)
                            + sample
                                .classifications
                                .iter()
                                .map(|classification| {
                                    token_count(classification.kind.label()) + token_count(&classification.reason)
                                })
                                .sum::<usize>()
                            + 1
                    })
                    .sum::<usize>(),
            )
    }

    pub fn summary(&self) -> String {
        let mut languages = self
            .files
            .iter()
            .map(|file| file.language.display_label())
            .collect::<Vec<_>>();
        languages.sort_unstable();
        languages.dedup();
        if languages.is_empty() || (languages.len() == 1 && languages[0] == SourceLanguage::Rust.display_label()) {
            format!(
                "Analyzed {} Rust source files and recorded {} omitted paths within the selected source scope.",
                self.inventory.analyzed, self.inventory.omitted
            )
        } else {
            format!(
                "Analyzed {} source files ({}) and recorded {} omitted paths within the selected source scope.",
                self.inventory.analyzed,
                languages.join(", "),
                self.inventory.omitted
            )
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageCapability {
    pub language: SourceLanguage,
    pub extensions: Vec<String>,
    pub grammar: String,
    pub grammar_version: String,
    pub query_pack: String,
    pub query_pack_version: String,
    pub definitions: bool,
    pub references: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapCollections {
    pub files: CollectionSummary,
    pub symbols: CollectionSummary,
    pub omissions: CollectionSummary,
    pub findings: CollectionSummary,
    pub edges: CollectionSummary,
    pub ranking: CollectionSummary,
    pub snippets: CollectionSummary,
    #[serde(default)]
    pub landmarks: CollectionSummary,
    #[serde(default)]
    pub project_roots: CollectionSummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Landmark {
    pub kind: LandmarkKind,
    pub path: String,
    pub reason: String,
    pub project_root: Option<String>,
    pub worktree_state: WorktreeState,
    pub priority: u8,
    #[serde(default)]
    pub focus_matches: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectRoot {
    pub path: String,
    pub kind: ProjectRootKind,
    pub reason: String,
    pub manifests: Vec<String>,
    #[serde(default)]
    pub landmark_total: usize,
    #[serde(default)]
    pub recommendation_total: usize,
    #[serde(default)]
    pub recommended_paths: Vec<String>,
}

/// One file-level lexical dependency candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LexicalEdge {
    pub source: String,
    pub target: String,
    pub symbol: String,
    pub ambiguous: bool,
    /// All lexical definition candidates for this reference.
    pub candidates: Vec<String>,
    /// Stable identity shared by all edges in one deduplicated candidate group.
    #[serde(default)]
    pub candidate_group: String,
    #[serde(default)]
    pub resolution_reason: LexicalResolutionReason,
    #[serde(default)]
    pub confidence: ConfidenceTier,
    #[serde(default)]
    pub target_visibility: SymbolVisibility,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileRank {
    pub path: String,
    /// PageRank plus explicit-focus score scaled by 1,000,000.
    pub score: u64,
    pub focus_matches: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapSelection {
    pub token_budget: usize,
    pub estimated_tokens: usize,
    pub snippets: Vec<MapSnippet>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapSnippet {
    pub path: String,
    pub language: SourceLanguage,
    pub symbol: SourceSymbol,
    /// Symbol score scaled by 1,000,000.
    pub score: u64,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapCacheReport {
    pub mode: CacheMode,
    pub status: CacheStatus,
    /// Number of normalized `--cache-file` names that matched eligible paths.
    #[serde(default)]
    pub matched: usize,
    /// Number of normalized `--cache-file` names that matched no eligible path.
    #[serde(default)]
    pub unmatched: usize,
    /// Number of eligible paths that could not be analyzed because this mode
    /// did not permit a cache refresh and no usable record was available.
    #[serde(default)]
    pub unavailable: usize,
    pub hits: usize,
    pub misses: usize,
    pub refreshed: Vec<String>,
    pub stale: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapInventory {
    pub tracked: usize,
    pub modified: usize,
    pub untracked: usize,
    pub analyzed: usize,
    pub omitted: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClassificationKind {
    Generated,
    Vendor,
    Minified,
    SourceMap,
}

impl SourceClassificationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Vendor => "vendor",
            Self::Minified => "minified",
            Self::SourceMap => "source_map",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceClassification {
    pub kind: SourceClassificationKind,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceClassificationSample {
    pub path: String,
    pub classifications: Vec<SourceClassification>,
    pub overridden: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapClassificationSummary {
    /// Number of unique paths classified before parser/cache analysis.
    pub total: usize,
    /// Number of bounded path samples returned in `samples`.
    pub returned: usize,
    pub truncated: bool,
    #[serde(default)]
    pub reason: Option<TruncationReason>,
    pub generated: usize,
    pub vendor: usize,
    pub minified: usize,
    pub source_map: usize,
    pub samples: Vec<SourceClassificationSample>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub language: SourceLanguage,
    pub extension: String,
    pub worktree_state: WorktreeState,
    pub status: FileAnalysisStatus,
    pub symbols: Vec<SourceSymbol>,
    pub limitations: Vec<String>,
    #[serde(default)]
    pub classifications: Vec<SourceClassification>,
    #[serde(default)]
    pub classification_overridden: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub role: SymbolRole,
    pub scope: Vec<String>,
    pub location: SourceLocation,
    pub context: String,
    #[serde(default)]
    pub visibility: SymbolVisibility,
    #[serde(default)]
    pub evidence: SymbolEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExplainReport {
    pub target: String,
    pub target_kind: ExplainTargetKind,
    pub matched_paths: Vec<String>,
    pub matched_symbols: Vec<ExplainSymbolMatch>,
    pub focus_matches: Vec<String>,
    pub history_overlap: Vec<PathCount>,
    pub landmark: Option<ExplainLandmark>,
    pub ranking: Vec<ExplainRanking>,
    pub graph_edges: Vec<LexicalEdge>,
    pub ambiguity: Vec<MapFinding>,
    pub omitted_alternatives: Vec<SourceOmission>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExplainSymbolMatch {
    pub path: String,
    pub symbol: SourceSymbol,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExplainRanking {
    pub path: String,
    pub score: u64,
    pub focus_matches: usize,
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExplainLandmark {
    pub kind: String,
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceLocation {
    pub start: Position,
    pub end: Position,
}

impl From<tree_sitter::Node<'_>> for SourceLocation {
    fn from(node: tree_sitter::Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        SourceLocation {
            start: Position { line: start.row + 1, column: start.column + 1 },
            end: Position { line: end.row + 1, column: end.column + 1 },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceOmission {
    pub path: String,
    pub reason: OmissionReason,
    pub detail: String,
    #[serde(default)]
    pub classifications: Vec<SourceClassification>,
    #[serde(default)]
    pub classification_overridden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapFinding {
    pub kind: MapFindingKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    pub detail: String,
}
