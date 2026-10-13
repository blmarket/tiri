//! Agent integration, split out of the server that first hosted it.
//!
//! A server hosts this crate by implementing [`observability::v1`] over its own
//! panes and ticking an [`integration::AgentObserver`] on its event loop; the
//! observer publishes per-pane [`integration::status::AgentStatus`] into a
//! [`integration::status::StatusHub`] the server's format layer reads back.
//! Nothing here knows what server it runs in, which is what lets both the tiri
//! daemon and the transpiled tmux server carry the same detectors.

pub mod integration;
pub mod observability;
pub mod pane_class;
pub mod platform;
