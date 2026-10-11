//! Compatibility names for input components; pane decoding uses its owner trait.
pub(crate) use crate::src::window_pane::{
    input_complete_request, input_free_request, input_request_matches,
};
pub use crate::src::window_pane::{
    input_csi_type, input_esc_type, input_init, input_parse_screen, input_pending,
    input_reply_clipboard, input_request_reply, input_reset, input_set_buffer_size,
    input_table_entry, InputRequestReply, INPUT_BUF_START, INPUT_CSI_CBT, INPUT_CSI_CNL,
    INPUT_CSI_CPL, INPUT_CSI_CUB, INPUT_CSI_CUD, INPUT_CSI_CUF, INPUT_CSI_CUP, INPUT_CSI_CUU,
    INPUT_CSI_DA, INPUT_CSI_DA_TWO, INPUT_CSI_DCH, INPUT_CSI_DECSCUSR, INPUT_CSI_DECSTBM,
    INPUT_CSI_DL, INPUT_CSI_DSR, INPUT_CSI_DSR_PRIVATE, INPUT_CSI_ECH, INPUT_CSI_ED, INPUT_CSI_EL,
    INPUT_CSI_HPA, INPUT_CSI_ICH, INPUT_CSI_IL, INPUT_CSI_MODOFF, INPUT_CSI_MODSET,
    INPUT_CSI_QUERY, INPUT_CSI_QUERY_PRIVATE, INPUT_CSI_RCP, INPUT_CSI_REP, INPUT_CSI_RM,
    INPUT_CSI_RM_PRIVATE, INPUT_CSI_SCP, INPUT_CSI_SD, INPUT_CSI_SGR, INPUT_CSI_SM,
    INPUT_CSI_SM_GRAPHICS, INPUT_CSI_SM_PRIVATE, INPUT_CSI_SU, INPUT_CSI_TBC, INPUT_CSI_VPA,
    INPUT_CSI_WINOPS, INPUT_CSI_XDA, INPUT_DISCARD, INPUT_END_BEL, INPUT_END_ST, INPUT_ESC_DECALN,
    INPUT_ESC_DECKPAM, INPUT_ESC_DECKPNM, INPUT_ESC_DECRC, INPUT_ESC_DECSC, INPUT_ESC_HTS,
    INPUT_ESC_IND, INPUT_ESC_NEL, INPUT_ESC_RI, INPUT_ESC_RIS, INPUT_ESC_SCSG0_OFF,
    INPUT_ESC_SCSG0_ON, INPUT_ESC_SCSG1_OFF, INPUT_ESC_SCSG1_ON, INPUT_ESC_ST, INPUT_LAST,
    INPUT_REQUEST_TIMEOUT,
};

use crate::src::shared::pane::window_pane;
use crate::src::window_pane::WindowPane;

pub unsafe fn input_parse_pane(pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    pane.parse_input();
}
pub unsafe fn input_parse_buffer(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    bytes: &[u8],
) {
    if !bytes.is_empty() {
        pane.parse_output(bytes);
    }
}
