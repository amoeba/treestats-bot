-- Initial schema for bot database
-- This table logs commands triggered by the Discord bot

CREATE TABLE IF NOT EXISTS command_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command_name TEXT NOT NULL,
    user_id TEXT NOT NULL,
    user_name TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    guild_id TEXT,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    message_id TEXT NOT NULL,
    success BOOLEAN NOT NULL DEFAULT 1,
    error_message TEXT
);

-- Index for querying by command name
CREATE INDEX IF NOT EXISTS idx_command_logs_command_name ON command_logs(command_name);

-- Index for querying by user
CREATE INDEX IF NOT EXISTS idx_command_logs_user_id ON command_logs(user_id);

-- Index for querying by timestamp
CREATE INDEX IF NOT EXISTS idx_command_logs_timestamp ON command_logs(timestamp DESC);
