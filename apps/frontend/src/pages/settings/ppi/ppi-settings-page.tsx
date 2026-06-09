import { useState, useEffect } from "react";
import { Separator } from "@wealthfolio/ui/components/ui/separator";
import { Button } from "@wealthfolio/ui/components/ui/button";
import { Input } from "@wealthfolio/ui/components/ui/input";
import { Label } from "@wealthfolio/ui/components/ui/label";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import {
  savePpiCredentials,
  deletePpiCredentials,
  getPpiCredentialsStatus,
  syncPpiData,
} from "@/adapters";
import { SettingsHeader } from "../settings-header";

export default function PpiSettingsPage() {
  const [apiKey, setApiKey] = useState("");
  const [apiSecret, setApiSecret] = useState("");
  const [authorizedClient, setAuthorizedClient] = useState("");
  const [clientKey, setClientKey] = useState("");
  const [isConfigured, setIsConfigured] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState(() => new Date().toISOString().slice(0, 10));

  useEffect(() => {
    getPpiCredentialsStatus()
      .then(setIsConfigured)
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    if (!apiKey.trim() || !apiSecret.trim() || !authorizedClient.trim() || !clientKey.trim()) {
      toast({ title: "Campos incompletos", description: "Todos los campos son requeridos." });
      return;
    }
    setIsSaving(true);
    try {
      await savePpiCredentials(apiKey.trim(), apiSecret.trim(), authorizedClient.trim(), clientKey.trim());
      setIsConfigured(true);
      setApiKey("");
      setApiSecret("");
      setAuthorizedClient("");
      setClientKey("");
      toast({ title: "Guardado", description: "Credenciales PPI guardadas." });
    } catch (e) {
      toast({ title: "Error", description: `No se pudieron guardar las credenciales: ${e}` });
    } finally {
      setIsSaving(false);
    }
  };

  const handleSync = async () => {
    setIsSyncing(true);
    try {
      await syncPpiData(startDate || undefined, endDate || undefined);
      toast({ title: "Sincronización iniciada", description: "Revisá las actividades cuando termine." });
    } catch (e) {
      toast({ title: "Error de sincronización", description: `${e}` });
    } finally {
      setIsSyncing(false);
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deletePpiCredentials();
      setIsConfigured(false);
      toast({ title: "Eliminado", description: "Credenciales PPI eliminadas." });
    } catch (e) {
      toast({ title: "Error", description: `No se pudieron eliminar las credenciales: ${e}` });
    } finally {
      setIsDeleting(false);
    }
  };

  const placeholder = isConfigured ? "••••••••" : undefined;

  return (
    <div className="space-y-6">
      <SettingsHeader
        heading="Portfolio Personal (PPI)"
        text="Conectá tu cuenta PPI para sincronizar operaciones y posiciones."
      />
      <Separator />

      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <div
            className={`h-2 w-2 rounded-full ${isConfigured ? "bg-green-500" : "bg-muted-foreground"}`}
          />
          <span className="text-sm">
            {isConfigured ? "Credenciales configuradas" : "Sin credenciales configuradas"}
          </span>
        </div>

        <div className="space-y-4 rounded-lg border p-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="ppi-api-key">API Key (pública)</Label>
              <Input
                id="ppi-api-key"
                type="password"
                placeholder={placeholder ?? "Key pública"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                autoComplete="off"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="ppi-api-secret">API Secret (privada)</Label>
              <Input
                id="ppi-api-secret"
                type="password"
                placeholder={placeholder ?? "Key privada"}
                value={apiSecret}
                onChange={(e) => setApiSecret(e.target.value)}
                autoComplete="off"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="ppi-authorized-client">AuthorizedClient</Label>
              <Input
                id="ppi-authorized-client"
                type="password"
                placeholder={placeholder ?? "AuthorizedClient"}
                value={authorizedClient}
                onChange={(e) => setAuthorizedClient(e.target.value)}
                autoComplete="off"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="ppi-client-key">ClientKey</Label>
              <Input
                id="ppi-client-key"
                type="password"
                placeholder={placeholder ?? "ClientKey"}
                value={clientKey}
                onChange={(e) => setClientKey(e.target.value)}
                autoComplete="off"
              />
            </div>
          </div>

          <div className="flex gap-2">
            <Button onClick={handleSave} disabled={isSaving}>
              {isSaving && <Icons.Spinner className="mr-2 size-4 animate-spin" />}
              Guardar credenciales
            </Button>
            {isConfigured && (
              <Button variant="destructive" onClick={handleDelete} disabled={isDeleting}>
                {isDeleting && <Icons.Spinner className="mr-2 size-4 animate-spin" />}
                Eliminar
              </Button>
            )}
          </div>
        </div>

        {isConfigured && (
          <div className="space-y-4 rounded-lg border p-4">
            <p className="text-muted-foreground text-sm">
              Sincronizá actividades y posiciones desde PPI. La sincronización corre en segundo
              plano y emite eventos al completarse.
            </p>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-1">
                <Label htmlFor="ppi-start-date">Desde (opcional)</Label>
                <Input
                  id="ppi-start-date"
                  type="date"
                  value={startDate}
                  onChange={(e) => setStartDate(e.target.value)}
                />
                <p className="text-muted-foreground text-xs">
                  Vacío trae solo lo reciente.
                </p>
              </div>
              <div className="space-y-1">
                <Label htmlFor="ppi-end-date">Hasta</Label>
                <Input
                  id="ppi-end-date"
                  type="date"
                  value={endDate}
                  onChange={(e) => setEndDate(e.target.value)}
                />
              </div>
            </div>
            <Button variant="outline" onClick={handleSync} disabled={isSyncing}>
              {isSyncing ? (
                <Icons.Spinner className="mr-2 size-4 animate-spin" />
              ) : (
                <Icons.RefreshCw className="mr-2 size-4" />
              )}
              Sincronizar ahora
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
