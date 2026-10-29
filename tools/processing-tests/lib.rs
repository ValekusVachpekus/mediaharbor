#[path = "../../crates/core/src/engine.rs"]
pub mod engine;
#[path = "../../crates/core/src/ipc.rs"]
pub mod ipc;
#[path = "../../crates/core/src/media.rs"]
pub mod media;
#[path = "../../crates/core/src/model.rs"]
pub mod model;
#[path = "../../crates/core/src/process.rs"]
pub mod process;
#[path = "../../crates/core/src/store.rs"]
pub mod store;

// Explicit test double at the network boundary. Processing, SQLite, scheduling,
// protocol and IPC below are the production modules; Spotify HTTP/OAuth is not.
pub mod spotify {
    use crate::model::MediaCandidate;
    use anyhow::{bail, Result};
    use serde_json::Value;
    #[derive(Default)]
    pub struct Spotify;
    pub async fn title(_: &str) -> Result<MediaCandidate> {
        bail!("Spotify is unavailable in offline processing tests")
    }
    impl Spotify {
        pub fn connected(&self, _: &str) -> bool {
            false
        }
        pub fn logout(&self, _: &str) -> Result<()> {
            Ok(())
        }
        pub async fn begin(self: &std::sync::Arc<Self>, _: &str) -> Result<Value> {
            bail!("offline")
        }
        pub async fn collection(&self, _: &str, _: &str, _: usize) -> Result<Value> {
            bail!("offline")
        }
    }
}

pub mod updates {
    pub async fn check() -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Update service is unavailable in offline tests")
    }
}
