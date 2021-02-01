use crate::Ring;

use std::sync::Arc;

#[derive(Debug)]
pub struct SubmissionQueue {
    ring: Arc<Ring>,
}

impl SubmissionQueue {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }
}
