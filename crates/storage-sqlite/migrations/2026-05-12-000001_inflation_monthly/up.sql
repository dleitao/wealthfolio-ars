CREATE TABLE inflation_monthly (
  period       TEXT NOT NULL PRIMARY KEY,  -- "2024-03" (YYYY-MM)
  monthly_rate REAL NOT NULL,
  source       TEXT NOT NULL DEFAULT 'ARGENTINA_DATOS',
  fetched_at   TEXT NOT NULL
);
