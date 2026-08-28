//! Wire protocol for the custom Cast Receiver (`client/src/pages/receiver`).
//!
//! Sent via `rust_cast::channels::receiver::ReceiverChannel::broadcast_message`
//! (`crate::chromecast`) and received on the JS side via
//! `context.addCustomMessageListener(CAST_NAMESPACE, cb)` -- the receiver
//! page fetches everything else it needs (transcript, audio stems,
//! background asset) itself, same-origin against this server, once it has
//! `file_hash`. Mirrored by hand as `CAST_NAMESPACE` in
//! `client/src/lib/cast/protocol.ts` -- ts-rs exports types, not const
//! values, so keep the two namespace strings in sync manually.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const CAST_NAMESPACE: &str = "urn:x-cast:com.nightingale.karaoke";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum CastReceiverMessage {
    Load {
        file_hash: String,
        /// 0.0-1.0 guide-vocal mix level; omitted -> receiver falls back to
        /// its own fetched `AppConfig.guide_volume`.
        #[serde(skip_serializing_if = "Option::is_none")]
        guide_volume: Option<f64>,
    },
}
