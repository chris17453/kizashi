-- White-label branding (spec §1: "white-labelable and multi-tenant"). Nullable columns, not a
-- separate table -- one optional row of metadata per tenant, no join needed to read it back
-- alongside the tenant lookup the login page already does. NULL means "use the platform
-- default", not "unset and broken" -- every reader must treat these as optional overrides.
ALTER TABLE tenants ADD COLUMN product_name TEXT;
ALTER TABLE tenants ADD COLUMN logo_url TEXT;
ALTER TABLE tenants ADD COLUMN accent_color TEXT;
