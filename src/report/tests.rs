use super::*;

use crate::report::{CommandDescriptor, Finding, Limitation, ReportScope, ReportStatus, SCHEMA_VERSION};
use std::path::PathBuf;

#[test]
fn markdown_escapes_report_content_that_could_add_control_sequences() {
    let report = Report {
        schema_version: SCHEMA_VERSION,
        profile: AnalysisProfile::Compact,
        limits: ReportLimits::for_profile(AnalysisProfile::Compact),
        command: CommandDescriptor::map(PathBuf::from("unsafe\u{1b}[31m-path")),
        scope: ReportScope { selected_path: "unsafe\u{1b}[31m-path".to_owned() },
        status: ReportStatus::Foundation,
        summary: "A\u{1b}[31m summary".to_owned(),
        provenance: ReportProvenance::default(),
        quality: ReportQuality::default(),
        findings: vec![Finding { title: "title*".to_owned(), detail: "detail\u{7}".to_owned() }],
        limitations: vec![Limitation { detail: "limitation\u{1b}[0m".to_owned() }],
        reading_plan: None,
        history: None,
        map: None,
        explain: None,
    };

    let markdown = report.render(OutputFormat::Markdown).expect("markdown renders");
    assert!(!markdown.contains('\u{1b}'));
    assert!(!markdown.contains('\u{7}'));
    assert!(markdown.contains("title\\*"));
}

#[test]
fn schema_and_golden_v1_corpus_cover_all_report_variants() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../schema/v1/codeplat.json")).expect("schema is valid JSON");
    assert_eq!(
        schema["$defs"]["analysis_report"]["properties"]["schema_version"]["const"],
        1
    );
    assert!(
        schema["$defs"]["analysis_report"]["required"]
            .as_array()
            .expect("analysis required fields")
            .iter()
            .any(|field| field == "command")
    );
    assert!(schema["$defs"]["analysis_report"]["properties"]["reading_plan"].is_object());

    let analysis = [
        include_str!("../../schema/v1/golden/briefing.json"),
        include_str!("../../schema/v1/golden/map.json"),
        include_str!("../../schema/v1/golden/history.json"),
    ];
    for document in analysis {
        let report: Report = serde_json::from_str(document).expect("historical v1 report remains readable");
        assert_eq!(report.schema_version, SCHEMA_VERSION);
    }
    let capabilities: CapabilitiesReport =
        serde_json::from_str(include_str!("../../schema/v1/golden/capabilities.json"))
            .expect("capabilities golden remains readable");
    assert_eq!(capabilities.schema_version, SCHEMA_VERSION);
    let doctor: DoctorReport = serde_json::from_str(include_str!("../../schema/v1/golden/doctor.json"))
        .expect("doctor golden remains readable");
    assert_eq!(doctor.schema_version, SCHEMA_VERSION);
    assert!(!doctor.source_evidence_collected);
    assert!(!doctor.repository_state_changed);
}
