# Homelab Docker — Fork Argentino

Guía para buildear y desplegar el fork (con PPI/Balanz/ARS providers) en un
homelab con Docker Compose.

---

## 1. Buildear la imagen

Desde la raíz del repo:

```bash
docker build -t wealthfolio-ar:latest .
```

Para arm64 (Raspberry Pi, Apple Silicon en homelab, etc.) en una máquina amd64:

```bash
docker buildx build \
  --platform linux/arm64 \
  -t wealthfolio-ar:latest \
  --load \
  .
```

> El Dockerfile usa `tonistiigi/xx` para cross-compilación, así que el target
> puede diferir del host sin problemas.

---

## 2. Generar secretos

### `WF_SECRET_KEY` (obligatorio)

Clave de 32 bytes que protege el secret store (credenciales PPI, etc.) y firma
los tokens JWT. Generarla una sola vez y guardarla; si cambia, el secret store
queda ilegible.

```bash
openssl rand -base64 32
# Ejemplo de salida: K7gNU3sdo+OL0wNhqoVWhr3g6s1xYv72ol/pe/Unols=
```

### `WF_AUTH_PASSWORD_HASH` (obligatorio si la app es accesible desde la red)

Hash Argon2id de tu contraseña de acceso a la UI:

```bash
# Reemplazar "mi-contraseña" y "salt16caracteres" por valores propios
printf 'mi-contraseña' | argon2 salt16caracteres! -id -e
# Ejemplo de salida: $argon2id$v=19$m=65536,t=3,p=4$c2FsdDE2Y2FyYWN0ZXJlcyE$...
```

En el `.env` de Docker Compose, los `$` deben escaparse como `$$`:

```
WF_AUTH_PASSWORD_HASH=$$argon2id$$v=19$$m=65536,t=3,p=4$$...
```

Si usás Authelia, Authentik u otro proxy que ya maneja autenticación, podés
omitir la contraseña y pasar `WF_AUTH_REQUIRED=false`.

---

## 3. `docker-compose.yml`

```yaml
services:
  wealthfolio:
    image: wealthfolio-ar:latest
    container_name: wealthfolio
    restart: unless-stopped
    ports:
      - "8088:8088"
    volumes:
      - wealthfolio_data:/data
    env_file:
      - wealthfolio.env

volumes:
  wealthfolio_data:
```

---

## 4. `wealthfolio.env`

Crear este archivo junto al `docker-compose.yml` (nunca commitearlo):

```env
# --- Obligatorio ---
# Clave de 32 bytes en base64 (generar con: openssl rand -base64 32)
WF_SECRET_KEY=REEMPLAZAR_CON_TU_CLAVE_BASE64

# Puerto y path de la DB
WF_LISTEN_ADDR=0.0.0.0:8088
WF_DB_PATH=/data/wealthfolio.db

# --- Autenticación (recomendado) ---
# Hash Argon2id de tu contraseña. Escapar $ como $$ en archivos .env
WF_AUTH_PASSWORD_HASH=$$argon2id$$v=19$$m=65536,t=3,p=4$$SALT$$HASH
# Dominio desde donde accedés (obligatorio si auth está activo; no puede ser *)
WF_CORS_ALLOW_ORIGINS=https://wealthfolio.tu-dominio.com
# TTL del token de sesión en minutos (default: 60)
# WF_AUTH_TOKEN_TTL_MINUTES=1440

# Si un proxy externo maneja la auth, desactivar el chequeo interno:
# WF_AUTH_REQUIRED=false

# --- Opcional ---
# WF_LOG_FORMAT=json
# WF_LOGS_DIR=/data/logs
```

---

## 5. Configurar el conector PPI

Las credenciales de PPI (API key, API secret) se guardan **cifradas en la DB**,
no en variables de entorno. El flujo es:

1. Levantar el stack: `docker compose up -d`
2. Abrir la UI en `http://tu-servidor:8088`
3. Ir a **Settings → PPI** e ingresar el API key y API secret.
4. La app los almacena en el secret store cifrado con `WF_SECRET_KEY`.

> **Importante:** si cambiás `WF_SECRET_KEY` después de haber guardado
> credenciales, el secret store queda ilegible. Esas credenciales deberán
> ingresarse nuevamente.

---

## 6. Levantar

```bash
# Primera vez — crear el directorio si usás bind mount en vez de named volume
# (no es necesario con named volumes)
# mkdir -p ./data && sudo chown -R 1000:1000 ./data

docker compose up -d
docker compose logs -f wealthfolio
```

La app estará disponible en `http://tu-servidor:8088`.

---

## 7. Actualizar

```bash
# Rebuildar desde el repo
docker build -t wealthfolio-ar:latest .

# Recrear el contenedor (la DB en /data persiste)
docker compose up -d --force-recreate
```

---

## Referencia rápida de variables

| Variable | Default | Descripción |
|---|---|---|
| `WF_SECRET_KEY` | — (obligatorio) | Base64 de 32 bytes. Protege secret store y JWT. |
| `WF_DB_PATH` | `./db/app.db` | Ruta del archivo SQLite. |
| `WF_LISTEN_ADDR` | `0.0.0.0:8088` | Dirección de escucha. |
| `WF_AUTH_PASSWORD_HASH` | — | Hash Argon2id. Requerido en red no-loopback. |
| `WF_AUTH_REQUIRED` | `true` | Poner `false` si el proxy maneja auth. |
| `WF_CORS_ALLOW_ORIGINS` | `*` | Orígenes CORS. Debe ser explícito si auth activo. |
| `WF_COOKIE_SECURE` | `auto` | `auto` / `true` / `false`. |
| `WF_AUTH_TOKEN_TTL_MINUTES` | `60` | Duración del token de sesión. |
| `WF_LOG_FORMAT` | `text` | `text` o `json`. |
| `WF_LOGS_DIR` | junto a la DB | Directorio de logs. |
| `CONNECT_API_URL` | — | URL del backend Connect (si aplica). |
| `CONNECT_AUTH_URL` | — | URL de auth Connect (build arg o runtime). |
| `CONNECT_AUTH_PUBLISHABLE_KEY` | — | Publishable key Connect (build arg o runtime). |
