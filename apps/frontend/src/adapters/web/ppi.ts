// Web-specific PPI adapter.
// Credentials are stored/retrieved via the existing /secrets HTTP endpoint.
// Sync uses the dedicated /ppi/sync endpoint added to the web server.
import { invoke } from "./core";

const KEYS = ["ppi_api_key", "ppi_api_secret", "ppi_authorized_client", "ppi_client_key"] as const;

export const savePpiCredentials = async (
  apiKey: string,
  apiSecret: string,
  authorizedClient: string,
  clientKey: string,
): Promise<void> => {
  const values = [apiKey, apiSecret, authorizedClient, clientKey];
  for (let i = 0; i < KEYS.length; i++) {
    await invoke<void>("set_secret", { secretKey: KEYS[i], secret: values[i] });
  }
};

export const deletePpiCredentials = async (): Promise<void> => {
  for (const key of [...KEYS, "ppi_refresh_token"]) {
    try {
      await invoke<void>("delete_secret", { secretKey: key });
    } catch {
      // ignore — secret may not exist
    }
  }
};

export const getPpiCredentialsStatus = async (): Promise<boolean> => {
  for (const key of KEYS) {
    const val = await invoke<string | null>("get_secret", { secretKey: key });
    if (!val) return false;
  }
  return true;
};

export const syncPpiData = async (startDate?: string, endDate?: string): Promise<void> => {
  return invoke<void>("sync_ppi_data", { startDate, endDate });
};
