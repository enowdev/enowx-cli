//! Terminal interface with reference-inspired chrome and a paged telemetry sidebar.
mod ansi;
mod app;
mod attachments;
mod commands;
mod modal;
mod pricing;
mod runtime;
mod session;
mod text;
pub mod theme;
mod ui;

pub mod testing;

pub use runtime::run;
