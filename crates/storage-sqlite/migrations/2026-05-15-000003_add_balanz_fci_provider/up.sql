INSERT OR IGNORE INTO market_data_providers (id, name, description, url, priority, enabled, logo_filename, last_synced_at, last_sync_status, last_sync_error)
VALUES
    ('BALANZ_FCI', 'Balanz FCI', 'Provides daily VCP (valor de cuotaparte) for Balanz Capital mutual funds (BCACCA, BCAHA, INSTITUA, OPPORA). No authentication required.', 'https://balanz.com/', 8, TRUE, NULL, NULL, NULL, NULL);
