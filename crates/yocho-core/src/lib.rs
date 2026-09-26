//! Yochō core types (ADR-0200, ADR-0205, ADR-0208): the signal schema every Yochō stage
//! (extractors, detectors, scorer, signal writer) exchanges over the `signal.emitted` bus
//! message, plus the minimal stage traits. This crate holds no I/O and never sees encryption
//! keys — provenance ciphertext is produced and consumed elsewhere (`yocho-crypto`).

pub mod bus;
pub mod entity_ref;
pub mod provenance;
pub mod signal;
pub mod signal_id;
pub mod signal_value;
pub mod stage;
pub mod validation;

pub use bus::SIGNAL_EMITTED_EXCHANGE;
pub use entity_ref::EntityRef;
pub use provenance::{Contribution, EncryptedBlob, Provenance};
pub use signal::Signal;
pub use signal_id::{ParseSignalIdError, SignalId};
pub use signal_value::SignalValue;
pub use stage::{Detector, Extractor, Scorer, StageError};
pub use validation::{SignalError, MAX_DIMS, MAX_DIM_KEY_LEN, MAX_DIM_VALUE_LEN, MAX_IDENT_LEN};
