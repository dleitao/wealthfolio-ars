-- Disable providers that are not used in this deployment.
-- Active providers: PPI, ARGENTINA_DATOS, DOLAR_API, BALANZ_FCI, CUSTOM_SCRAPER.
UPDATE market_data_providers
SET enabled = FALSE
WHERE id IN ('US_TREASURY_CALC', 'OPENFIGI', 'YAHOO', 'MARKETDATA_APP', 'ALPHA_VANTAGE', 'FINNHUB', 'BOERSE_FRANKFURT', 'METAL_PRICE_API');
