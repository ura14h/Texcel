mod events;
mod input;
pub(super) mod pass_focus;
mod sync;

/// Separates owned IME text from ordinary navigation for one frame.
pub(super) struct ImeFrameState {
    pub(super) navigation_blocked: bool,
    pub(super) text_event: bool,
    pub(super) commit_before_pointer: bool,
}
