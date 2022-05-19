//! Generic db abstration.

pub mod mock;
#[cfg(test)]
mod tests;
pub mod transaction;

use std::time::Duration;

use cadence::{Gauged, StatsdClient};
use syncstorage_common::Metrics;
use syncstorage_db_common::{
    error::{DbError, DbErrorKind},
    results, DbPool, GetPoolState, PoolState,
};
use syncstorage_mysql::pool::MysqlDbPool;
use syncstorage_settings::Settings;
use syncstorage_spanner::pool::SpannerDbPool;
use tokio::{self, time};
use url::Url;

/// Create/initialize a pool of managed Db connections
pub async fn pool_from_settings(
    settings: &Settings,
    metrics: &Metrics,
) -> Result<Box<dyn DbPool>, DbError> {
    let url =
        Url::parse(&settings.database_url).map_err(|e| DbErrorKind::InvalidUrl(e.to_string()))?;
    Ok(match url.scheme() {
        "mysql" => Box::new(MysqlDbPool::new(settings, metrics)?),
        "spanner" => Box::new(SpannerDbPool::new(settings, metrics).await?),
        _ => Err(DbErrorKind::InvalidUrl(settings.database_url.to_owned()))?,
    })
}

/// Emit DbPool metrics periodically
pub fn spawn_pool_periodic_reporter<T: GetPoolState + Send + 'static>(
    interval: Duration,
    metrics: StatsdClient,
    pool: T,
) -> Result<(), DbError> {
    let hostname = hostname::get()
        .expect("Couldn't get hostname")
        .into_string()
        .expect("Couldn't get hostname");
    tokio::spawn(async move {
        loop {
            let PoolState {
                connections,
                idle_connections,
            } = pool.state();
            metrics
                .gauge_with_tags(
                    "storage.pool.connections.active",
                    (connections - idle_connections) as u64,
                )
                .with_tag("hostname", &hostname)
                .send();
            metrics
                .gauge_with_tags("storage.pool.connections.idle", idle_connections as u64)
                .with_tag("hostname", &hostname)
                .send();
            time::delay_for(interval).await;
        }
    });

    Ok(())
}
