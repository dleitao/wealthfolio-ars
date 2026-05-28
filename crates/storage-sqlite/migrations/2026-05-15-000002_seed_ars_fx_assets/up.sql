INSERT OR IGNORE INTO assets (kind, name, display_code, instrument_type, instrument_symbol, quote_ccy, quote_mode)
VALUES
    ('FX', 'ARS CCL / USD Exchange Rate',      'ARS_CCL/USD',       'FX', 'ARS_CCL',       'USD', 'MARKET'),
    ('FX', 'ARS Blue / USD Exchange Rate',      'ARS_BLUE/USD',      'FX', 'ARS_BLUE',      'USD', 'MARKET'),
    ('FX', 'ARS Mayorista / USD Exchange Rate', 'ARS_MAYORISTA/USD', 'FX', 'ARS_MAYORISTA', 'USD', 'MARKET'),
    ('FX', 'ARS Tarjeta / USD Exchange Rate',   'ARS_TARJETA/USD',   'FX', 'ARS_TARJETA',   'USD', 'MARKET'),
    ('FX', 'ARS Cripto / USD Exchange Rate',    'ARS_CRIPTO/USD',    'FX', 'ARS_CRIPTO',    'USD', 'MARKET');
