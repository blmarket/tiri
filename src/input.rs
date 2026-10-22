//! Compatibility names for input components; pane decoding uses its owner trait.
pub(crate) use crate::window_pane::{
    input_cancel_observed_requests, input_reply_observed_requests,
};
pub use crate::window_pane::{
    input_init, input_parse_screen, input_pending, input_request_reply, input_set_buffer_size,
    InputRequestReply,
};
