// Argentina-specific commands: inflation data
import { invoke, logger } from "./platform";

export interface InflationRecord {
  period: string; // "YYYY-MM"
  monthlyRate: number;
  source: string;
  fetchedAt: string;
}

export const syncInflationData = async (): Promise<number> => {
  try {
    return await invoke<number>("sync_inflation_data");
  } catch (err) {
    logger.error("Error syncing inflation data.");
    throw err;
  }
};

export const getInflationData = async (): Promise<InflationRecord[]> => {
  try {
    return await invoke<InflationRecord[]>("get_inflation_data");
  } catch (err) {
    logger.error("Error fetching inflation data.");
    throw err;
  }
};

export const syncArgentinaSectors = async (): Promise<[number, number]> => {
  try {
    return await invoke<[number, number]>("sync_argentina_sectors");
  } catch (err) {
    logger.error("Error syncing Argentina sectors.");
    throw err;
  }
};
