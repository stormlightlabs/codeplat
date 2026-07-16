use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryReport {
    pub repository_root: String,
    pub scope_path: String,
    #[serde(default)]
    pub head: HeadSnapshot,
    #[serde(default)]
    pub provenance: HistoryProvenance,
    pub settings: HistorySettings,
    pub commits_seen: usize,
    pub non_merge_commits_seen: usize,
    #[serde(default)]
    pub collections: HistoryCollections,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn: Option<ChurnReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<ContributorReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bugs: Option<BugReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<ActivityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firefighting: Option<FirefightingReport>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryCollections {
    pub commits: CollectionSummary,
    pub churn_paths: CollectionSummary,
    pub contributor_identity_mappings: CollectionSummary,
    pub contributors_overall: CollectionSummary,
    pub contributors_recent: CollectionSummary,
    pub bug_paths: CollectionSummary,
    pub bug_overlap_paths: CollectionSummary,
    pub bug_commits: CollectionSummary,
    pub activity_months: CollectionSummary,
    pub firefighting_commits: CollectionSummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChurnReport {
    pub window_days: u32,
    #[serde(default)]
    pub size_basis: String,
    #[serde(default)]
    pub rename_continuity: RenameContinuity,
    pub paths: Vec<PathCount>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenameContinuity {
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContributorReport {
    pub recent_window_days: u32,
    #[serde(default)]
    pub mailmap_applied: bool,
    #[serde(default)]
    pub identity_mappings: Vec<IdentityMapping>,
    pub overall: Vec<ContributorCount>,
    pub recent: Vec<ContributorCount>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct IdentityMapping {
    pub raw_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_email: Option<String>,
    pub canonical_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_email: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContributorCount {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub commits: usize,
    pub share_percent: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BugReport {
    pub window_days: u32,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub keyword_match: KeywordMatchMode,
    pub paths: Vec<PathCount>,
    pub overlap_paths: Vec<PathCount>,
    pub commits: Vec<CommitEvidence>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityReport {
    pub months: Vec<MonthlyActivity>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MonthlyActivity {
    pub month: String,
    pub commits: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirefightingReport {
    pub window_days: u32,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub keyword_match: KeywordMatchMode,
    pub commits: Vec<CommitEvidence>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitEvidence {
    pub id: String,
    pub subject: String,
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_terms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PathCount {
    pub path: String,
    pub commits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commits_per_kib_milli: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_status: Option<String>,
}
