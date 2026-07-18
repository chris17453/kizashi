//! Fabric SQL analytics endpoint connector (ADR-0003, ADR-0013). No
//! `tests/raw_record_contract_test.rs` exists here, unlike every other connector — that test
//! asserts poll() output conforms to the `RawRecord` schema by actually calling `poll` and
//! checking a successful result, which requires a server that accepts an Entra AAD token login
//! (real Fabric, or an Azure SQL-compatible service); no such server is available to test
//! against here, and a plain TDS server (used in `tests/fabric_connector_integration_test.rs`
//! to prove the real connection/auth/error-classification path) always rejects AAD token
//! logins. The row-to-JSON mapping itself is structurally identical to the `sql` connector's
//! already-tested `row_to_json`, which is the part of this connector `RawRecord` schema
//! conformance would actually be checking.

mod connector;

pub use connector::FabricConnector;
