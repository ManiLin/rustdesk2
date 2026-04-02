# Портал учёта клиентов RustDesk

Отдельный сервис: API на Rust (Axum + SQLite), веб-интерфейс на TypeScript/React (Vite), Docker Compose.

## Запуск

```bash
cd inventory_portal
export INVENTORY_DEVICE_TOKEN="$(openssl rand -hex 24)"
export ADMIN_PASSWORD="$(openssl rand -hex 16)"
export JWT_SECRET="$(openssl rand -hex 32)"
docker compose up -d --build
```

Веб: `http://localhost:8088` (порт задаётся переменной `PORT`).

## GitHub Actions

У **Flutter Nightly Build** и **Flutter Tag Build** при ручном запуске есть поле **inventory-report-url**. Оно передаётся в сборку как `INVENTORY_REPORT_URL` и **вшивается в клиент**.  
По расписанию или при push тега поле пустое — URL в бинарник не попадает.

Приоритет на клиенте: значение из **`RustDesk2.toml`** (`inventory-report-url`), если пусто — зашитый при сборке URL.

Локальная сборка: `INVENTORY_REPORT_URL='https://…/api/v1/report' cargo build …`

## Настройка RustDesk

В конфигурации клиента (или при сборке через встроенные опции):

| Ключ | Значение |
|------|----------|
| `inventory-report-url` | Полный URL (перекрывает URL, зашитый при сборке) |
| `inventory-report-token` | Необязательно: если пусто, клиент использует **`RS_PUB_KEY`** из `config.rs` (константа `DEFAULT_INVENTORY_REPORT_TOKEN`) |

Отправка включается при **непустом URL**. Токен по умолчанию совпадает с **`RS_PUB_KEY`** вашей сборки; на портале переменная **`INVENTORY_DEVICE_TOKEN`** в `docker-compose` должна быть тем же значением (в репозитории задан тот же дефолт).

Интервал: первый отчёт через ~15 с, далее каждые **5 минут**.

**Важно:** используйте HTTPS на границе (обратный прокси). Временный пароль и идентификаторы — чувствительные данные.

## API

- `POST /api/v1/report` — заголовок `Authorization: Bearer <INVENTORY_DEVICE_TOKEN>`, тело JSON (см. `inventory_sync.rs`).
- `POST /api/v1/auth/login` — `{ "password": "<ADMIN_PASSWORD>" }` → JWT.
- `GET /api/v1/devices` — заголовок `Authorization: Bearer <jwt>`.
