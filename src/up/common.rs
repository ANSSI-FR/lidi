// SPDX-License-Identifier: LGPL-3.0

use serde::{Deserialize, Serialize};

use crate::datagram::RandomId;

#[derive(Serialize, Deserialize)]
pub struct TransferMessage {
    pub random_id: RandomId,
}
