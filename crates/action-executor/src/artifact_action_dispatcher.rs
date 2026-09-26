#[path = "artifact_action_dispatcher_test.rs"]
#[cfg(test)]
mod artifact_action_dispatcher_test;

use crate::action_dispatcher::{ActionDispatcher, DispatchError};
use async_trait::async_trait;
use common::{ActionRef, ActionType, Event};

pub struct ArtifactActionDispatcher {
    egress_proxy_url: Option<String>,
}

impl ArtifactActionDispatcher {
    pub fn new(egress_proxy_url: Option<String>) -> Self {
        Self { egress_proxy_url }
    }
}

fn required_url(action: &ActionRef) -> Result<&str, DispatchError> {
    action
        .config
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(DispatchError::MissingUrl)
}

fn rendered_text(action: &ActionRef, event: &Event) -> String {
    let default = format!(
        "Kizashi response artifact\nEvent type: {}\nEntity: {}\nGroup: {}\nPayload: {}",
        event.event_type, event.entity_ref, event.group_key, event.payload
    );
    let template = action.config.get("body_template").and_then(serde_json::Value::as_str);
    template
        .unwrap_or(&default)
        .replace("{{event_type}}", &event.event_type)
        .replace("{{entity_ref}}", &event.entity_ref)
        .replace("{{group_key}}", &event.group_key)
        .replace("{{tenant_id}}", &event.tenant_id.to_string())
        .replace("{{payload}}", &event.payload.to_string())
}

fn pdf_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

fn build_pdf(text: &str) -> Vec<u8> {
    let lines = text.lines().take(80).collect::<Vec<_>>();
    let mut stream = String::from("BT\n/F1 10 Tf\n50 760 Td\n");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            stream.push_str("0 -14 Td\n");
        }
        stream.push_str(&format!("({}) Tj\n", pdf_escape(line)));
    }
    stream.push_str("ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
    ];
    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = output.len();
    output.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref
        )
        .as_bytes(),
    );
    output
}

fn xml_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn build_xlsx(text: &str) -> Vec<u8> {
    let rows = text
        .lines()
        .take(200)
        .map(|line| {
            format!("<Row><Cell><Data ss:Type=\"String\">{}</Data></Cell></Row>", xml_escape(line))
        })
        .collect::<String>();
    format!("<?xml version=\"1.0\"?><Workbook xmlns=\"urn:schemas-microsoft-com:office:spreadsheet\" xmlns:ss=\"urn:schemas-microsoft-com:office:spreadsheet\"><Worksheet ss:Name=\"Kizashi\"><Table>{rows}</Table></Worksheet></Workbook>").into_bytes()
}

#[async_trait]
impl ActionDispatcher for ArtifactActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let url = required_url(action)?;
        let (format, content_type, bytes) = match action.action_type {
            ActionType::GeneratePdf => {
                ("pdf", "application/pdf", build_pdf(&rendered_text(action, event)))
            }
            ActionType::GenerateXlsx => {
                ("xlsx", "application/vnd.ms-excel", build_xlsx(&rendered_text(action, event)))
            }
            _ => {
                return Err(DispatchError::InvalidConfig(
                    "artifact dispatcher received a non-artifact action".to_string(),
                ))
            }
        };
        let client = common::build_outbound_client(
            self.egress_proxy_url.as_deref(),
            event.tenant_id,
            "action-executor",
        )
        .map_err(|error| DispatchError::Unreachable(error.to_string()))?;
        let response = client
            .post(url)
            .header("content-type", content_type)
            .body(bytes.clone())
            .send()
            .await
            .map_err(|error| DispatchError::Unreachable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(DispatchError::Rejected(response.status().as_u16()));
        }
        Ok(
            serde_json::json!({"http_status": response.status().as_u16(), "artifact_format": format, "bytes": bytes.len()}),
        )
    }
}
