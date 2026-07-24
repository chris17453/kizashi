use super::*;

#[test]
fn report_run_starts_in_running_state_with_csv_artifact() {
    let run = ReportRun::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "weekly",
        "ops@example.com",
        "/reports/export.csv",
    );
    assert_eq!(run.status, "running");
    assert_eq!(run.format, "csv");
    assert!(run.artifact_url.is_some());
}
