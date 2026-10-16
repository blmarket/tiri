//! Compatibility names for input components; pane decoding uses its owner trait.
pub(crate) use crate::src::window_pane::{
    input_complete_request, input_free_request, input_request_matches,
};
pub use crate::src::window_pane::{
    input_init, input_parse_screen, input_pending, input_request_reply, input_set_buffer_size,
    InputRequestReply,
};
