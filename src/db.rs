use anyhow::{Context, Result};
use sqlx::ConnectOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tracing::info;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

#[derive(Debug)]
pub struct CommandLog {
    pub command_name: String,
    pub user_id: String,
    pub user_name: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub message_id: String,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Recent log entry for queries
#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct RecentLog {
    pub command_name: String,
    pub user_name: String,
    pub timestamp: i64,
    pub success: bool,
}

/// User statistics
#[allow(dead_code)]
#[derive(Debug)]
pub struct UserStats {
    pub user_id: String,
    pub total_count: i64,
    pub command_breakdown: Vec<(String, i64)>,
    pub first_use: Option<i64>,
    pub last_use: Option<i64>,
}

/// Daily usage statistics
#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct DailyUsage {
    pub date: String,
    pub count: i64,
}

impl Database {
    pub async fn init() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            info!("DATABASE_URL not set, using default: ./bot.db");
            "sqlite:./bot.db".to_string()
        });

        info!("Connecting to database: {}", database_url);

        // Parse connection options
        let mut options = SqliteConnectOptions::from_str(&database_url)
            .context("Failed to parse DATABASE_URL")?
            .create_if_missing(true);

        // Disable logging of SQL statements (too verbose)
        options = options.disable_statement_logging();

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("Failed to connect to database")?;

        info!("Database connected successfully");

        let db = Self { pool };

        // Just always run migrations on init
        db.migrate().await?;

        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        info!("Running database migrations...");

        // Read and execute the initial migration
        // TODO: Refactor to run migrations in order
        let migration_sql = include_str!("./migrations/20251128_initial.sql");

        sqlx::query(migration_sql)
            .execute(&self.pool)
            .await
            .context("Failed to run migrations")?;

        info!("Database migrations completed successfully");
        Ok(())
    }

    pub async fn log_command(&self, log: CommandLog) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO command_logs (command_name, user_id, user_name, channel_id, guild_id, message_id, success, error_message)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#
        )
        .bind(&log.command_name)
        .bind(&log.user_id)
        .bind(&log.user_name)
        .bind(&log.channel_id)
        .bind(&log.guild_id)
        .bind(&log.message_id)
        .bind(log.success)
        .bind(&log.error_message)
        .execute(&self.pool)
        .await
        .context("Failed to log command")?;

        Ok(())
    }

    /// Get command statistics
    #[allow(dead_code)]
    pub async fn get_command_stats(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT command_name, COUNT(*) as count
            FROM command_logs
            WHERE success = 1
            GROUP BY command_name
            ORDER BY count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch command stats")?;

        Ok(rows)
    }

    /// Get recent command logs
    #[allow(dead_code)]
    pub async fn get_recent_logs(&self, limit: i64) -> Result<Vec<RecentLog>> {
        let rows = sqlx::query_as::<_, RecentLog>(
            r#"
            SELECT
                command_name,
                user_name,
                timestamp,
                success
            FROM command_logs
            ORDER BY timestamp DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch recent logs")?;

        Ok(rows)
    }

    /// Get total number of successful command uses
    #[allow(dead_code)]
    pub async fn get_total_uses(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM command_logs WHERE success = 1
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch total uses")?;

        Ok(count)
    }

    /// Get command usage count for a specific user
    #[allow(dead_code)]
    pub async fn get_user_command_count(&self, user_id: &str) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM command_logs WHERE user_id = ?1 AND success = 1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch user command count")?;

        Ok(count)
    }

    /// Get detailed usage statistics for a user
    #[allow(dead_code)]
    pub async fn get_user_stats(&self, user_id: &str) -> Result<UserStats> {
        // Get total count
        let total_count = self.get_user_command_count(user_id).await?;

        // Get per-command breakdown
        let command_breakdown = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT command_name, COUNT(*) as count
            FROM command_logs
            WHERE user_id = ?1 AND success = 1
            GROUP BY command_name
            ORDER BY count DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch user command breakdown")?;

        // Get first and last usage timestamps
        let (first_use, last_use): (Option<i64>, Option<i64>) = sqlx::query_as(
            r#"
            SELECT
                MIN(timestamp) as first_use,
                MAX(timestamp) as last_use
            FROM command_logs
            WHERE user_id = ?1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch user timestamps")?;

        Ok(UserStats {
            user_id: user_id.to_string(),
            total_count,
            command_breakdown,
            first_use,
            last_use,
        })
    }

    /// Get usage statistics over time (daily counts)
    #[allow(dead_code)]
    pub async fn get_usage_over_time(&self, days: i64) -> Result<Vec<DailyUsage>> {
        let cutoff = chrono::Utc::now().timestamp() - (days * 86400);

        let rows = sqlx::query_as::<_, DailyUsage>(
            r#"
            SELECT
                date(timestamp, 'unixepoch') as date,
                COUNT(*) as count
            FROM command_logs
            WHERE success = 1
              AND timestamp >= ?1
            GROUP BY date(timestamp, 'unixepoch')
            ORDER BY date DESC
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch usage over time")?;

        Ok(rows)
    }
}
