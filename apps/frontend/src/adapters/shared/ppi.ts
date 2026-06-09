// PPI broker commands
import { invoke, logger } from "./platform";

export const savePpiCredentials = async (
  apiKey: string,
  apiSecret: string,
  authorizedClient: string,
  clientKey: string,
): Promise<void> => {
  try {
    return await invoke<void>("save_ppi_credentials", {
      apiKey,
      apiSecret,
      authorizedClient,
      clientKey,
    });
  } catch (err) {
    logger.error("Error saving PPI credentials.");
    throw err;
  }
};

export const deletePpiCredentials = async (): Promise<void> => {
  return invoke<void>("delete_ppi_credentials");
};

export const getPpiCredentialsStatus = async (): Promise<boolean> => {
  try {
    return await invoke<boolean>("get_ppi_credentials_status");
  } catch (err) {
    logger.error("Error checking PPI credentials status.");
    throw err;
  }
};

export const syncPpiData = async (startDate?: string, endDate?: string): Promise<void> => {
  try {
    return await invoke<void>("sync_ppi_data", {
      startDate: startDate ?? null,
      endDate: endDate ?? null,
    });
  } catch (err) {
    logger.error("Error syncing PPI data.");
    throw err;
  }
};
