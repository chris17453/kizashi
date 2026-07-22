use common::ontology::{ActionType, LinkType, ObjectType};
use common::{RawRecord, SourceType};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/kizashi".into());
    let pool = common::connect_with_schema(&database_url, "ontology_service").await.unwrap();

    let tenant_id = Uuid::new_v4();

    println!("Creating Zendesk Ticket ObjectType...");
    let ticket_type_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO object_types (id, tenant_id, name, version, property_schema, mapping_rules)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        ticket_type_id,
        tenant_id,
        "ZendeskTicket",
        1,
        json!({
            "type": "object",
            "properties": {
                "ticket_id": {"type": "string"},
                "subject": {"type": "string"}
            }
        }),
        json!([
            {
                "source_type": "ticket",
                "fields": {
                    "ticket_id": "id",
                    "subject": "subject"
                }
            }
        ])
    )
    .execute(&pool)
    .await
    .unwrap();

    println!("Creating Zendesk User ObjectType...");
    let user_type_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO object_types (id, tenant_id, name, version, property_schema, mapping_rules)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        user_type_id,
        tenant_id,
        "ZendeskUser",
        1,
        json!({
            "type": "object",
            "properties": {
                "user_id": {"type": "string"},
                "email": {"type": "string"}
            }
        }),
        json!([
            {
                "source_type": "ticket",
                "fields": {
                    "user_id": "requester_id"
                }
            }
        ])
    )
    .execute(&pool)
    .await
    .unwrap();

    println!("Creating LinkType Ticket -> User...");
    let link_type_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO link_types (id, tenant_id, name, source_object_type_id, target_object_type_id, cardinality)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        link_type_id,
        tenant_id,
        "requested_by",
        ticket_type_id,
        user_type_id,
        "many_to_one"
    )
    .execute(&pool)
    .await
    .unwrap();

    println!("Creating ActionType...");
    let action_type_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO action_types (id, tenant_id, name, parameter_schema, preconditions, effect_definition)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        action_type_id,
        tenant_id,
        "escalate_ticket",
        json!({
            "type": "object",
            "properties": {
                "priority": {"type": "string"}
            }
        }),
        json!({}),
        json!({"action": "call_external_api", "endpoint": "/escalate"})
    )
    .execute(&pool)
    .await
    .unwrap();

    println!("Done. Run ontology-service to test mappings via RabbitMQ or via integration test.");
}
