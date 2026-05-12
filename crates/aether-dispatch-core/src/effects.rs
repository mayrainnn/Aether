#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DispatchEffectKind {
    CandidateFailed,
    RateLimited,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchEffect {
    pub kind: DispatchEffectKind,
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: Option<String>,
    pub candidate_index: u32,
    pub reason: Option<String>,
}
