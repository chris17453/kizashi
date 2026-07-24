INSERT INTO action_type_history (id, tenant_id, action_type_id, change_type, actor, before_state, after_state, changed_at)
SELECT gen_random_uuid(), tenant_id, id, 'created', 'system-backfill', NULL,
       jsonb_build_object(
           'id', id,
           'tenant_id', tenant_id,
           'name', name,
           'target_object_type_id', target_object_type_id,
           'parameter_schema', parameter_schema,
           'preconditions', preconditions,
           'effect_definition', effect_definition,
           'created_at', created_at,
           'updated_at', updated_at
       ),
       created_at
FROM action_types
WHERE NOT EXISTS (
    SELECT 1 FROM action_type_history history
    WHERE history.tenant_id = action_types.tenant_id
      AND history.action_type_id = action_types.id
);
