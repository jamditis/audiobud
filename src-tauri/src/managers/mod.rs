pub mod audio;
pub mod history;
pub mod model;
pub mod personalization;
pub mod transcription;
// Pure watchdog machinery, split out of transcription.rs so CI (which swaps
// transcription.rs for transcription_mock.rs) still compiles and tests it.
pub mod watchdog;
