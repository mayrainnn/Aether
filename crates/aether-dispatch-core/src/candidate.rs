#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderEndpointRef {
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub api_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyRef {
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub api_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PoolRef {
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub api_format: String,
    pub pool_group_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchRankFacts {
    pub provider_priority: i32,
    pub key_priority: Option<i32>,
    pub ranking_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DispatchCandidateRef {
    SingleKey {
        key: KeyRef,
        rank: DispatchRankFacts,
    },
    PoolRef {
        pool: PoolRef,
        rank: DispatchRankFacts,
    },
}

impl DispatchCandidateRef {
    pub fn provider_endpoint(&self) -> ProviderEndpointRef {
        match self {
            Self::SingleKey { key, .. } => ProviderEndpointRef {
                provider_id: key.provider_id.clone(),
                endpoint_id: key.endpoint_id.clone(),
                model_id: key.model_id.clone(),
                selected_provider_model_name: key.selected_provider_model_name.clone(),
                api_format: key.api_format.clone(),
            },
            Self::PoolRef { pool, .. } => ProviderEndpointRef {
                provider_id: pool.provider_id.clone(),
                endpoint_id: pool.endpoint_id.clone(),
                model_id: pool.model_id.clone(),
                selected_provider_model_name: pool.selected_provider_model_name.clone(),
                api_format: pool.api_format.clone(),
            },
        }
    }
}
