#[test]
fn pipeline_outbox_uses_the_shared_topic_exchange() {
    assert_eq!(common::PIPELINE_EXECUTION_EXCHANGE, "pipeline.execution");
}
