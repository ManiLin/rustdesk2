# Портал учёта клиентов RustDesk

Отдельный сервис: API на Rust (Axum + SQLite), веб-интерфейс на TypeScript/React (Vite), Docker Compose.

## Запуск

```bash
cd inventory_portal
export INVENTORY_DEVICE_TOKEN="$(openssl rand -hex 24)"
export ADMIN_PASSWORD="$(openssl rand -hex 16)"
export JWT_SECRET="$(openssl rand -hex 32)"
# AD -> адресная книга (токен rustdeskweb живёт только на портале)
export RUSTDESKWEB_API_URL="https://rustdeskweb.corp.tatnefturs.ru"
export RUSTDESKWEB_API_TOKEN="<admin api-token rustdeskweb>"
docker compose up -d --build
```

Веб: `http://localhost:8088` (порт задаётся переменной `PORT`).

## AD → адресная книга

Клиент **больше не хранит токен rustdeskweb**. Он отправляет свою AD-идентичность
на портал, а портал сам добавляет/обновляет запись в общей адресной книге.

- `POST /api/v1/ad/assign` — заголовок `Authorization: Bearer <INVENTORY_DEVICE_TOKEN>`,
  тело: `{ "rustdesk_id", "ad_domain", "ad_user", "display_name", "username", "hostname", "platform" }`.
  Ответ: `{ "status": "assigned" | "skipped" | "error", "message": ... }`.
- Портал использует `RUSTDESKWEB_API_URL` / `RUSTDESKWEB_API_TOKEN`, домен `AD_DOMAIN`
  и коллекцию `PRESET_ADDRESS_BOOK_NAME`.
- Если `RUSTDESKWEB_API_TOKEN` не задан — назначение отключено (ответ `disabled`).
- На клиенте URL портала берётся из `inventory-report-url` (база), токен — из
  `inventory-report-token` или `RS_PUB_KEY`.

## Сборки и обновления (с подтверждением администратора)

Сборки больше не публикуются сразу: загрузка создаёт запись со статусом `pending`,
и только администратор в UI подтверждает её (`published`) — тогда клиенты её видят.

- **Flavor**: `normal` и `cashdesk` (у cashdesk свой канал обновлений).
- **Платформа**: `windows` (основной канал), а также `linux`/`macos`/`android`.
- Загрузка (админ): `POST /api/v1/admin/builds` — `multipart/form-data`:
  `version`, `flavor`, `platform`, `file` (sha256 считается на сервере).
- Загрузка из CI: `POST /api/v1/ci/builds` — тот же формат, заголовок
  `Authorization: Bearer <CI_UPLOAD_TOKEN>` (если пусто — берётся
  `INVENTORY_DEVICE_TOKEN`). Скрипт: `.github/scripts/upload-build-to-portal.sh`.
- Список: `GET /api/v1/admin/builds` (JWT).
- Подтверждение: `POST /api/v1/admin/builds/{id}/approve` — публикует сборку,
  предыдущая опубликованная для того же flavor/platform уходит в `archived`.
- Отклонение: `POST /api/v1/admin/builds/{id}/reject`.
- Публичный meta (клиент): `GET /api/v1/downloads/rustdesk/windows/meta?flavor=normal|cashdesk`
  → `{ available, version, download_path, sha256 }` только для подтверждённой сборки.
- Публичное скачивание: `GET /api/v1/downloads/rustdesk/windows/latest?flavor=...`.
- Файлы хранятся в `UPLOAD_DIR/builds/` (по умолчанию `/data/downloads/builds`).
- Лимит размера — `MAX_UPLOAD_BYTES` (512 МБ по умолчанию); `client_max_body_size`
  у nginx и `DefaultBodyLimit` у Axum уже настроены.

Клиент сам подставляет свой flavor в meta-запрос (`cashdesk`/`normal`), платформа — `windows`.

## GitHub Actions

У **Flutter Nightly Build** и **Flutter Tag Build** при ручном запуске есть поле **inventory-report-url**. Оно передаётся в сборку как `INVENTORY_REPORT_URL` и **вшивается в клиент**.  
По расписанию или при push тега поле пустое — URL в бинарник не попадает.

Приоритет на клиенте: значение из **`RustDesk2.toml`** (`inventory-report-url`), если пусто — зашитый при сборке URL.

Можно указать только базу, например `http://192.168.0.213:1026` или с слэшем в конце — клиент сам допишет путь **`/api/v1/report`**.

Локальная сборка: `INVENTORY_REPORT_URL='https://…' cargo build …`

## Настройка RustDesk

В конфигурации клиента (или при сборке через встроенные опции):

| Ключ | Значение |
|------|----------|
| `inventory-report-url` | Полный URL (перекрывает URL, зашитый при сборке) |
| `inventory-report-token` | Необязательно: если пусто, клиент использует **`RS_PUB_KEY`** из `config.rs` (константа `DEFAULT_INVENTORY_REPORT_TOKEN`) |
| `inventory-update-meta-url` | Необязательно: полный URL `GET …/api/v1/downloads/rustdesk/windows/meta`. Если пусто, этот URL **выводится из** `inventory-report-url` (тот же хост, путь `api/v1/downloads/rustdesk/windows/meta`). |

### Автообновление Windows (exe) с портала

В **форке RustDesk** из этого репозитория: при непустом URL метаданных (см. выше) на **Windows** клиент ходит на портал вместо `api.rustdesk.com`, сравнивает версии и качает ваш `exe` (ветка MSI отключена для этого источника).

Нужно в клиенте:

- `enable-check-update` = `true`
- `allow-auto-update` = `true` — для фоновой проверки и установки; иначе доступна только ручная проверка из UI
- Корректный `inventory-report-url` (или отдельно `inventory-update-meta-url` на HTTPS в проде)

У **кастомного имени приложения** официальные обновления по-прежнему отключены, но **портал учёта** для exe остаётся доступен, если задан URL метаданных.

Отправка включается при **непустом URL**. Токен по умолчанию совпадает с **`RS_PUB_KEY`** вашей сборки; на портале переменная **`INVENTORY_DEVICE_TOKEN`** в `docker-compose` должна быть тем же значением (в репозитории задан тот же дефолт).

Интервал: первый отчёт через ~15 с, далее каждые **5 минут**.

**Важно:** используйте HTTPS на границе (обратный прокси). Временный пароль и идентификаторы — чувствительные данные.

## API

- `POST /api/v1/report` — заголовок `Authorization: Bearer <INVENTORY_DEVICE_TOKEN>`, тело JSON (см. `inventory_sync.rs`).
- `POST /api/v1/ad/assign` — заголовок `Authorization: Bearer <INVENTORY_DEVICE_TOKEN>`, AD → общая адресная книга (см. раздел выше).
- `POST /api/v1/auth/login` — `{ "password": "<ADMIN_PASSWORD>" }` → JWT.
- `GET /api/v1/devices` — заголовок `Authorization: Bearer ***
- `DELETE /api/v1/devices/{id}` — заголовок `Authorization: Bearer *** — удаление устройства из базы.
- `GET /api/v1/admin/builds` — список сборок (JWT).
- `POST /api/v1/admin/builds` — загрузка сборки (JWT, `multipart/form-data`: `version`, `flavor`, `platform`, `file`) → `pending`.
- `POST /api/v1/admin/builds/{id}/approve` — подтвердить сборку (JWT).
- `POST /api/v1/admin/builds/{id}/reject` — отклонить сборку (JWT).
- `POST /api/v1/ci/builds` — загрузка из CI (Bearer `CI_UPLOAD_TOKEN` или `INVENTORY_DEVICE_TOKEN`) → `pending`.
- `GET /api/v1/downloads/rustdesk/windows/meta?flavor=normal|cashdesk` — публичный JSON для клиентского автообновления.
- `GET /api/v1/downloads/rustdesk/windows/latest?flavor=...` — публичное скачивание подтверждённой сборки (поддерживает `HEAD`).
