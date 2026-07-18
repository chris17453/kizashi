#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

pub async fn healthz() -> &'static str {
    "ok"
}
