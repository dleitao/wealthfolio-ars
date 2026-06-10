DELETE FROM assets
WHERE instrument_type = 'FX'
  AND instrument_symbol IN ('ARS_CCL', 'ARS_BLUE', 'ARS_MAYORISTA', 'ARS_TARJETA', 'ARS_CRIPTO');
