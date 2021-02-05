use crate::Ring;

use std::sync::Arc;

#[derive(Debug)]
pub struct Registrar {
    ring: Arc<Ring>,
}

impl Registrar {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }
}
