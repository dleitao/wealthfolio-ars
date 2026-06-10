INSERT OR IGNORE INTO market_data_providers (id, name, description, url, priority, enabled, logo_filename, last_synced_at, last_sync_status, last_sync_error)
VALUES
    ('PPI', 'Portfolio Personal (PPI)', 'Provides real-time and historical quotes for Argentine instruments (BYMA/BCBA). Requires PPI API credentials configured in Settings.', 'https://portfoliopersonal.com/', 5, TRUE, NULL, NULL, NULL, NULL),
    ('ARGENTINA_DATOS', 'ArgentinaDatos', 'Provides historical ARS exchange rates (oficial, MEP) from the public ArgentinaDatos API. No authentication required.', 'https://argentinadatos.com/', 6, TRUE, NULL, NULL, NULL, NULL),
    ('DOLAR_API', 'DolarApi', 'Provides real-time ARS exchange rates (oficial, MEP/bolsa) from dolarapi.com. No authentication required.', 'https://dolarapi.com/', 7, TRUE, NULL, NULL, NULL, NULL);
