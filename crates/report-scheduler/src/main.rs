use axum::{routing::get, Router};
use chrono::{DateTime, Duration, Utc};
use common::ReportRun;
use lettre::message::{header::ContentType, Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::Deserialize;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use uuid::Uuid;

#[derive(Debug, Deserialize, Default)]
struct ScheduleFilter {
    #[serde(default)]
    frequency: String,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_format() -> String {
    "csv".into()
}

#[derive(Debug)]
struct Schedule {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    filter: ScheduleFilter,
}

#[derive(Debug, Clone)]
struct SmtpConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    from: String,
}

impl SmtpConfig {
    fn from_env() -> Option<Self> {
        let host =
            std::env::var("REPORT_SMTP_HOST").ok().filter(|value| !value.trim().is_empty())?;
        let from =
            std::env::var("REPORT_FROM_EMAIL").ok().filter(|value| !value.trim().is_empty())?;
        Some(Self {
            host,
            port: std::env::var("REPORT_SMTP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(587),
            username: std::env::var("REPORT_SMTP_USERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            password: std::env::var("REPORT_SMTP_PASSWORD")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            from,
        })
    }
}

#[derive(Debug, Deserialize)]
struct EventPage {
    events: Vec<EventRow>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct EventRow {
    id: Uuid,
    event_type: String,
    group_key: String,
    status: String,
    occurred_at: DateTime<Utc>,
}

fn interval_for(frequency: &str) -> Option<Duration> {
    match frequency {
        "daily" => Some(Duration::days(1)),
        "weekly" => Some(Duration::days(7)),
        "monthly" => Some(Duration::days(30)),
        _ => None,
    }
}

fn artifact_url(base: &str, filter: &ScheduleFilter) -> String {
    let mut params = vec![("from", filter.from.clone()), ("to", filter.to.clone())];
    params.retain(|(_, value)| !value.is_empty());
    let query = serde_urlencoded::to_string(params).unwrap_or_default();
    let extension = if filter.format == "pdf" { "pdf" } else { "csv" };
    format!(
        "{}/reports/export.{extension}{}",
        base.trim_end_matches('/'),
        if query.is_empty() { String::new() } else { format!("?{query}") }
    )
}

async fn load_schedules(pool: &PgPool) -> Result<Vec<Schedule>, sqlx::Error> {
    let rows: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT id, tenant_id, name, filter FROM saved_search_queries WHERE filter->>'view_kind' = 'report_schedule'",
    ).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, tenant_id, name, filter)| {
            let filter: ScheduleFilter = serde_json::from_value(filter).ok()?;
            Some(Schedule { id, tenant_id, name, filter })
        })
        .collect())
}

async fn latest_run(
    pool: &PgPool,
    schedule_id: Uuid,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar("SELECT started_at FROM report_runs WHERE schedule_id = $1 ORDER BY started_at DESC LIMIT 1")
        .bind(schedule_id).fetch_optional(pool).await
}

async fn persist_started(pool: &PgPool, run: &ReportRun) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO report_runs (id, tenant_id, schedule_id, schedule_name, recipient, format, status, error, artifact_url, started_at, completed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(run.id).bind(run.tenant_id).bind(run.schedule_id).bind(&run.schedule_name).bind(&run.recipient).bind(&run.format).bind(&run.status).bind(&run.error).bind(&run.artifact_url).bind(run.started_at).bind(run.completed_at).execute(pool).await.map(|_| ())
}

async fn persist_finished(pool: &PgPool, run: &ReportRun) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE report_runs SET status=$1, error=$2, artifact_url=$3, completed_at=$4 WHERE id=$5 AND tenant_id=$6")
        .bind(&run.status).bind(&run.error).bind(&run.artifact_url).bind(run.completed_at).bind(run.id).bind(run.tenant_id).execute(pool).await.map(|_| ())
}

async fn mint_token(
    client: &reqwest::Client,
    gateway: &str,
    secret: &str,
    tenant_id: Uuid,
) -> Result<String, String> {
    let response = client.post(format!("{gateway}/internal/tokens"))
        .header("x-internal-secret", secret)
        .json(&serde_json::json!({"tenant_id": tenant_id, "role": "operator", "label": "report-scheduler"}))
        .send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("token mint returned HTTP {}", response.status()));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?
        .get("token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "token mint response did not contain a token".to_string())
}

async fn fetch_report_data(
    client: &reqwest::Client,
    gateway: &str,
    token: &str,
    filter: &ScheduleFilter,
) -> Result<(String, Vec<EventRow>), String> {
    let mut offset = 0i64;
    let mut events = Vec::new();
    // The dashboard API caps a page at 1000. Keep the scheduled artifact bounded while still
    // consuming every page in the selected window instead of silently delivering one event.
    for _ in 0..20 {
        let mut request = client
            .get(format!("{gateway}/v1/events"))
            .bearer_auth(token)
            .query(&[("limit", "1000")])
            .query(&[("offset", offset)]);
        if !filter.from.is_empty() {
            request = request.query(&[("since", format!("{}T00:00:00Z", filter.from))]);
        }
        if !filter.to.is_empty() {
            request = request.query(&[("until", format!("{}T23:59:59Z", filter.to))]);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("event query returned HTTP {}", response.status()));
        }
        let page = response.json::<EventPage>().await.map_err(|e| e.to_string())?;
        let page_len = page.events.len() as i64;
        events.extend(page.events);
        if !page.has_more || page_len == 0 {
            break;
        }
        offset += page_len;
    }
    Ok((render_csv(&events), events))
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn render_csv(events: &[EventRow]) -> String {
    let mut csv = String::from("id,event_type,group_key,status,occurred_at\n");
    for event in events {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            event.id,
            csv_escape(&event.event_type),
            csv_escape(&event.group_key),
            event.status,
            event.occurred_at.to_rfc3339()
        ));
    }
    csv
}

fn pdf_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

fn render_pdf(events: &[EventRow]) -> Vec<u8> {
    let mut content =
        String::from("BT\n/F1 11 Tf\n50 760 Td\n(KIZASHI SCHEDULED REPORT) Tj\n0 -18 Td\n");
    content.push_str(&format!("(Signals in window: {}) Tj\n", events.len()));
    for event in events.iter().take(42) {
        content.push_str(&format!(
            "0 -15 Td\n({} | {} | {} | {}) Tj\n",
            pdf_escape(&event.event_type),
            pdf_escape(&event.group_key),
            event.status,
            event.occurred_at.to_rfc3339()
        ));
    }
    content.push_str("ET\n");
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
    ];
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    pdf
}

async fn deliver_email(
    config: SmtpConfig,
    recipient: String,
    schedule_name: String,
    attachment: Vec<u8>,
    filename: String,
    content_type: &'static str,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let from = config.from.parse().map_err(|e| format!("invalid report sender: {e}"))?;
        let to = recipient.parse().map_err(|e| format!("invalid report recipient: {e}"))?;
        let email = Message::builder()
            .from(from)
            .to(to)
            .subject(format!("Kizashi report: {schedule_name}"))
            .multipart(
                MultiPart::mixed()
                    .singlepart(SinglePart::plain(
                        "Your scheduled Kizashi report is attached.".to_string(),
                    ))
                    .singlepart(Attachment::new(filename).body(
                        attachment,
                        ContentType::parse(content_type).map_err(|e| e.to_string())?,
                    )),
            )
            .map_err(|e| e.to_string())?;
        let mut transport =
            SmtpTransport::relay(&config.host).map_err(|e| e.to_string())?.port(config.port);
        if let (Some(username), Some(password)) = (config.username, config.password) {
            transport = transport.credentials(Credentials::new(username, password));
        }
        transport.build().send(&email).map_err(|e| e.to_string()).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn run_due(
    pool: &PgPool,
    client: &reqwest::Client,
    gateway: &str,
    secret: &str,
    artifact_base: &str,
    smtp: Option<&SmtpConfig>,
) {
    let schedules = match load_schedules(pool).await {
        Ok(schedules) => schedules,
        Err(error) => {
            tracing::error!(%error, "failed to load report schedules");
            return;
        }
    };
    let now = Utc::now();
    for schedule in schedules {
        if !schedule.filter.enabled {
            continue;
        }
        let Some(interval) = interval_for(&schedule.filter.frequency) else {
            tracing::warn!(schedule_id = %schedule.id, "ignoring report schedule with invalid frequency");
            continue;
        };
        let latest = match latest_run(pool, schedule.id).await {
            Ok(latest) => latest,
            Err(error) => {
                tracing::error!(schedule_id = %schedule.id, %error, "failed to inspect report run history");
                continue;
            }
        };
        if latest.is_some_and(|started| now < started + interval) {
            continue;
        }

        let mut run = ReportRun::new(
            schedule.tenant_id,
            schedule.id,
            &schedule.name,
            &schedule.filter.recipient,
            artifact_url(artifact_base, &schedule.filter),
        );
        run.format = schedule.filter.format.clone();
        if let Err(error) = persist_started(pool, &run).await {
            tracing::error!(schedule_id = %schedule.id, %error, "failed to claim report schedule");
            continue;
        }
        let delivery = match mint_token(client, gateway, secret, schedule.tenant_id).await {
            Ok(token) => fetch_report_data(client, gateway, &token, &schedule.filter).await,
            Err(error) => Err(error),
        };
        match delivery {
            Ok((csv, events)) => {
                run.status = "generated".into();
                if let Some(smtp) = smtp {
                    let (attachment, filename, content_type) = if schedule.filter.format == "pdf" {
                        (
                            render_pdf(&events),
                            "operational-report.pdf".to_string(),
                            "application/pdf",
                        )
                    } else {
                        (csv.into_bytes(), "operational-report.csv".to_string(), "text/csv")
                    };
                    match deliver_email(
                        smtp.clone(),
                        schedule.filter.recipient.clone(),
                        schedule.name.clone(),
                        attachment,
                        filename,
                        content_type,
                    )
                    .await
                    {
                        Ok(()) => run.status = "delivered".into(),
                        Err(error) => {
                            run.status = "delivery_failed".into();
                            run.error = Some(error);
                        }
                    }
                }
            }
            Err(error) => {
                run.status = "failed".into();
                run.error = Some(error);
                run.artifact_url = None;
            }
        }
        run.completed_at = Some(Utc::now());
        if let Err(error) = persist_finished(pool, &run).await {
            tracing::error!(run_id = %run.id, %error, "failed to finalize scheduled report run");
        } else {
            tracing::info!(schedule_id = %schedule.id, status = %run.status, "scheduled report run completed");
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let gateway = std::env::var("QUERY_GATEWAY_URL").expect("QUERY_GATEWAY_URL must be set");
    let secret = std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let artifact_base = std::env::var("REPORT_ARTIFACT_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8093".into());
    let smtp = SmtpConfig::from_env();
    let interval = std::env::var("REPORT_SCHEDULER_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let pool = common::connect_with_schema(&database_url, "config_admin_service")
        .await
        .expect("failed to connect to config admin schema");
    let client = reqwest::Client::new();
    let loop_pool = pool.clone();
    let loop_client = client.clone();
    let loop_gateway = gateway.clone();
    let loop_secret = secret.clone();
    let loop_artifact = artifact_base.clone();
    let loop_smtp = smtp.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(StdDuration::from_secs(interval));
        loop {
            ticker.tick().await;
            run_due(
                &loop_pool,
                &loop_client,
                &loop_gateway,
                &loop_secret,
                &loop_artifact,
                loop_smtp.as_ref(),
            )
            .await;
        }
    });
    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.expect("bind failed");
    tracing::info!(%bind_addr, interval_seconds = interval, "report-scheduler listening");
    axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_windows_are_explicit_and_invalid_values_are_rejected() {
        assert_eq!(interval_for("daily"), Some(Duration::days(1)));
        assert_eq!(interval_for("weekly"), Some(Duration::days(7)));
        assert_eq!(interval_for("monthly"), Some(Duration::days(30)));
        assert_eq!(interval_for("hourly"), None);
    }

    #[test]
    fn artifact_url_preserves_the_schedule_window() {
        let filter = ScheduleFilter {
            frequency: "weekly".into(),
            from: "2026-07-01".into(),
            to: "2026-07-07".into(),
            ..Default::default()
        };
        assert_eq!(
            artifact_url("http://localhost:8093/", &filter),
            "http://localhost:8093/reports/export.csv?from=2026-07-01&to=2026-07-07"
        );
    }

    #[test]
    fn csv_output_quotes_comma_and_quote_values() {
        let event = EventRow {
            id: Uuid::nil(),
            event_type: "risk,alert".into(),
            group_key: "customer \"A\"".into(),
            status: "new".into(),
            occurred_at: DateTime::parse_from_rfc3339("2026-07-23T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        let csv = render_csv(&[event]);
        assert!(csv.contains("\"risk,alert\""));
        assert!(csv.contains("\"customer \"\"A\"\"\""));
    }

    #[tokio::test]
    async fn scheduled_report_fetch_paginates_the_event_window() {
        async fn handler(
            axum::extract::Query(query): axum::extract::Query<
                std::collections::HashMap<String, String>,
            >,
        ) -> axum::Json<serde_json::Value> {
            let offset =
                query.get("offset").and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
            let event = |id: u128| {
                serde_json::json!({
                    "id": Uuid::from_u128(id),
                    "event_type": "signal",
                    "group_key": "customer",
                    "status": "new",
                    "occurred_at": "2026-07-23T00:00:00Z"
                })
            };
            if offset == 0 {
                axum::Json(serde_json::json!({"events": [event(1), event(2)], "has_more": true}))
            } else {
                axum::Json(serde_json::json!({"events": [event(3)], "has_more": false}))
            }
        }
        let app = Router::new().route("/v1/events", get(handler));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let filter = ScheduleFilter {
            from: "2026-07-01".into(),
            to: "2026-07-23".into(),
            ..Default::default()
        };
        let (csv, events) = fetch_report_data(
            &reqwest::Client::new(),
            &format!("http://{address}"),
            "token",
            &filter,
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(csv.lines().count(), 4);
    }
}
