use async_trait::async_trait;
use sqlx::{mysql::MySqlRow, MySql, Row};

use super::{
    quota_snapshot_select, ProviderQuotaReadRepository, ProviderQuotaWriteRepository,
    StoredProviderQuotaSnapshot,
};
use crate::driver::mysql::MysqlPool;
use crate::error::SqlResultExt;
use crate::DataLayerError;
use aether_data_query::SqlDialect;

#[derive(Debug, Clone)]
pub struct MysqlProviderQuotaRepository {
    pool: MysqlPool,
}

impl MysqlProviderQuotaRepository {
    pub fn new(pool: MysqlPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProviderQuotaReadRepository for MysqlProviderQuotaRepository {
    async fn find_by_provider_id(
        &self,
        provider_id: &str,
    ) -> Result<Option<StoredProviderQuotaSnapshot>, DataLayerError> {
        let mut statement = quota_snapshot_select().statement::<MySql>(SqlDialect::Mysql);
        statement.where_eq("id", provider_id.to_string()).limit(1);
        let row = statement
            .finish()
            .build()
            .fetch_optional(&self.pool)
            .await
            .map_sql_err()?;
        row.as_ref().map(map_row).transpose()
    }

    async fn find_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderQuotaSnapshot>, DataLayerError> {
        if provider_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut statement = quota_snapshot_select().statement::<MySql>(SqlDialect::Mysql);
        statement
            .where_in("id", provider_ids)
            .order_by_sql("id ASC");
        let rows = statement
            .finish()
            .build()
            .fetch_all(&self.pool)
            .await
            .map_sql_err()?;
        rows.iter().map(map_row).collect()
    }
}

#[async_trait]
impl ProviderQuotaWriteRepository for MysqlProviderQuotaRepository {
    async fn reset_due(&self, now_unix_secs: u64) -> Result<usize, DataLayerError> {
        let now = i64::try_from(now_unix_secs).map_err(|_| {
            DataLayerError::InvalidInput("provider quota reset timestamp overflow".to_string())
        })?;
        let rows_affected = sqlx::query(
            r#"
UPDATE providers
SET monthly_used_usd = 0,
    quota_last_reset_at = ?,
    updated_at = ?
WHERE billing_type = 'monthly_quota'
  AND is_active = 1
  AND (
    quota_last_reset_at IS NULL
    OR (? - quota_last_reset_at) >= (quota_reset_day * 86400)
  )
"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_sql_err()?
        .rows_affected();
        Ok(usize::try_from(rows_affected).unwrap_or_default())
    }
}

fn map_row(row: &MySqlRow) -> Result<StoredProviderQuotaSnapshot, DataLayerError> {
    StoredProviderQuotaSnapshot::new(
        row.try_get("provider_id").map_sql_err()?,
        row.try_get("billing_type").map_sql_err()?,
        row.try_get("monthly_quota_usd").map_sql_err()?,
        row.try_get("monthly_used_usd").map_sql_err()?,
        row.try_get("quota_reset_day").map_sql_err()?,
        row.try_get("quota_last_reset_at_unix_secs").map_sql_err()?,
        row.try_get("quota_expires_at_unix_secs").map_sql_err()?,
        row.try_get("is_active").map_sql_err()?,
    )
}

#[cfg(test)]
mod tests {
    use super::MysqlProviderQuotaRepository;

    #[tokio::test]
    async fn repository_builds_from_lazy_pool() {
        let pool = sqlx::mysql::MySqlPoolOptions::new().connect_lazy_with(
            "mysql://user:pass@localhost:3306/aether"
                .parse()
                .expect("mysql options should parse"),
        );

        let _repository = MysqlProviderQuotaRepository::new(pool);
    }
}
