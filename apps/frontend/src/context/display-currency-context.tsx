import { createContext, useContext, useState, useCallback } from "react";

export type DisplayCurrency = "ARS" | "USD_OFICIAL" | "USD_MEP" | "USD_CCL";

interface DisplayCurrencyContextType {
  displayCurrency: DisplayCurrency;
  setDisplayCurrency: (currency: DisplayCurrency) => void;
}

const STORAGE_KEY = "wealthfolio_display_currency";

function loadSaved(): DisplayCurrency {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "ARS" || saved === "USD_OFICIAL" || saved === "USD_MEP" || saved === "USD_CCL") return saved;
  } catch {}
  return "ARS";
}

const DisplayCurrencyContext = createContext<DisplayCurrencyContextType | undefined>(undefined);

export function DisplayCurrencyProvider({ children }: { children: React.ReactNode }) {
  const [displayCurrency, setDisplayCurrencyState] = useState<DisplayCurrency>(loadSaved);

  const setDisplayCurrency = useCallback((currency: DisplayCurrency) => {
    setDisplayCurrencyState(currency);
    try {
      localStorage.setItem(STORAGE_KEY, currency);
    } catch {}
  }, []);

  return (
    <DisplayCurrencyContext.Provider value={{ displayCurrency, setDisplayCurrency }}>
      {children}
    </DisplayCurrencyContext.Provider>
  );
}

export function useDisplayCurrency() {
  const ctx = useContext(DisplayCurrencyContext);
  if (!ctx) throw new Error("useDisplayCurrency must be used within DisplayCurrencyProvider");
  return ctx;
}
