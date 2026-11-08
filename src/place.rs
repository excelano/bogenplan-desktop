// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub latitude: f64,
    pub longitude: f64,
}
