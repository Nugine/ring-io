use crate::Ring;

use std::sync::Arc;

#[derive(Debug)]
pub struct CompletionQueue {
    ring: Arc<Ring>,
}

impl CompletionQueue {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }
}
