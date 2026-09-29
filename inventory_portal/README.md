# TnursRemoteDeskWebApi

Отдельный сервис: API на Rust (Axum + SQLite), веб-интерфейс на TypeScript/React (Vite), Docker Compose.

## Запуск

```bash
cd inventory_portal
export INVENTORY_DEVICE_TOKEN="$(openssl rand -hex 24)"
export ADMIN_PASSWORD="$(openssl rand -hex 16)"
export JWT_SECRET="$(openssl rand -hex 32)"
# AD -> адресная книга (токен rustdeskweb живёт только на портале)
export RUSTDESKWEB_API_URL="https://tnremdeskapi.pxy2.tatnefturs.ru"
export RUSTDESKWEB_API_TOKEN="<admin api-token rustdeskweb>"
export PUBLIC_BASE_URL="https://tnremdeskapi.pxy2.tatnefturs.ru"
docker compose up -d --build
```

Веб: `http://localhost:1026` (порт задаётся переменной `PORT`, по умолчанию `1026`).

Адреса: снаружи сервис ничем себя не выдаёт — в корне и по любым неизвестным
путям отдаётся пустая 404-заглушка (`frontend/404.html`), версия nginx скрыта
(`server_tokens off`).

| Адрес | Что отдаётся |
| --- | --- |
| `/` | пустая 404-заглушка (никакого сайта в корне нет) |
| `/managment` | консоль управления (SPA) — единственная страница |
| `/management` | редирект на `/managment/` (защита от опечатки) |
| `/api/v1/...` | API портала, которое дёргают клиенты (nginx проксирует в backend `:8080`) |
| всё остальное | 404 без опознавательных признаков |

Консоль собрана с `base: "/managment/"`, поэтому в dev-режиме она открывается по
`http://localhost:5173/managment/` (`npm run dev` в `frontend/`).

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
- Удаление: `DELETE /api/v1/admin/builds/{id}` — удаляет сборку и её файл с портала (JWT).
- Публичный meta (клиент): `GET /api/v1/downloads/rustdesk/windows/meta?flavor=normal|cashdesk`
  → `{ available, version, download_path, sha256 }` только для подтверждённой сборки.
- Публичное скачивание: `GET /api/v1/downloads/rustdesk/windows/latest?flavor=...`.
- На Windows `latest` по умолчанию отдаёт portable EXE; `artifact_type=msi` выбирает установщик,
  `architecture=x86_64|aarch64|x86` — его архитектуру.
- Файлы хранятся в `UPLOAD_DIR/builds/` (по умолчанию `/data/downloads/builds`).
- Лимит размера — `MAX_UPLOAD_BYTES` (512 МБ по умолчанию); `client_max_body_size`
  у nginx и `DefaultBodyLimit` у Axum уже настроены.

Клиент сам подставляет свой flavor в meta-запрос (`cashdesk`/`normal`), платформа — `windows`.

### Установка Windows одной командой

В PowerShell на целевом устройстве выполните нужную команду. API определит архитектуру,
скачает опубликованный MSI, проверит SHA-256 и запустит тихую установку с запросом UAC:

```powershell
irm "https://tnremdeskapi.pxy2.tatnefturs.ru/api/v1/install/windows.ps1?flavor=normal" | iex
```

Для версии `forcash` укажите `flavor=cashdesk`:

```powershell
irm "https://tnremdeskapi.pxy2.tatnefturs.ru/api/v1/install/windows.ps1?flavor=cashdesk" | iex
```

Установка доступна после загрузки и подтверждения администратором MSI нужного flavor и архитектуры.
CI загружает отдельно MSI-установщик и EXE для автообновления.

## GitHub Actions

У **Flutter Nightly Build** и **Flutter Tag Build** при ручном запуске есть поле **inventory-report-url**. Оно передаётся в сборку как `INVENTORY_REPORT_URL` и **вшивается в клиент**.  
По расписанию или при push тега поле пустое — клиент использует встроенный адрес
по умолчанию `https://tnremdeskapi.pxy2.tatnefturs.ru`.

Приоритет на клиенте: значение из **`RustDesk2.toml`** (`inventory-report-url`), если пусто — зашитый при сборке URL.

Можно указать только базу, например `http://192.168.0.213:1026` или с слэшем в конце — клиент сам допишет путь **`/api/v1/report`**.

Локальная сборка может переопределить адрес: `INVENTORY_REPORT_URL='https://…' cargo build …`.
Если переменная не задана, используется тот же встроенный адрес по умолчанию.

## Общий реестр доп. серверов для тегов адресной книги

Клиент подключается к пиру с тегом (ключевым словом) через дополнительный
RustDesk-сервер, если для тега задан сервер: используется синтаксис ID
`peerId@host:port?key=<public key>`.

- **Реестр серверов (`host:port` + public key) хранится на портале** и раздаётся
  клиентам через `GET /api/v1/servers`; в клиенте список кэшируется
  (локальная опция `tag-rendezvous-server-list`), поэтому доступен и без портала.
- **Привязка «тег → сервер» остаётся локальной** (опция `tag-rendezvous-servers`)
  и задаётся в клиенте: правый клик по тегу → **Server** → сервер выбирается из
  выпадающего списка, public key подставляется автоматически.
- Общий список серверов меняет только администратор в разделе «Серверы» портала;
  клиенты могут читать список и кэшировать его локально. Публичный ключ обязателен.

## Web-интерфейс

Админ-портал разделён на разделы «Устройства», «Сборки» и «Серверы» и использует
компоненты Google Material Design 3 из закреплённой версии Material Web `2.5.0`.
Светлая тема используется по умолчанию; тёмная включается по настройке системы.
Временный пароль устройства скрыт, пока администратор явно его не раскроет.
Material Web сейчас находится в режиме поддержки; версия закреплена, чтобы
обновление компонентов было осознанным ([статус проекта](https://github.com/material-components/material-web)).

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
- `allow-auto-update` по умолчанию включено; значение `N` отключает фоновую установку, оставляя ручную проверку из UI
- Корректный `inventory-report-url` (или отдельно `inventory-update-meta-url` на HTTPS в проде)

У **кастомного имени приложения** официальные обновления по-прежнему отключены, но **портал учёта** для exe остаётся доступен, если задан URL метаданных.

Отправка включается при **непустом URL**. Токен по умолчанию совпадает с **`RS_PUB_KEY`** вашей сборки; на портале переменная **`INVENTORY_DEVICE_TOKEN`** в `docker-compose` должна быть тем же значением (в репозитории задан тот же дефолт).

Интервал: первый отчёт через ~15 с, далее каждые **5 минут**.

**Важно:** используйте HTTPS на границе (обратный прокси). Временный пароль и идентификаторы — чувствительные данные.

## API

Полная спецификация OpenAPI 3.1 находится в [`openapi.yaml`](openapi.yaml).
Все ответы с HTTP-ошибками имеют JSON-форму
`{ "code": "...", "message": "..." }`. Код `401` означает отсутствующие
или неверные данные входа; `403` — корректный токен устройства без прав
администратора; `409` — действие невозможно для текущего статуса сборки.
В успешных ответах v1 сохраняются действующие форматы.

- `POST /api/v1/report` — заголовок `Authorization: Bearer <INVENTORY_DEVICE_TOKEN>`, тело JSON (см. `inventory_sync.rs`).
- `POST /api/v1/ad/assign` — заголовок `Authorization: Bearer <INVEN...N>`, AD → общая адресная книга (см. раздел выше).
- `GET /api/v1/servers` — **общий реестр дополнительных RustDesk-серверов** (Bearer device-токен клиента или админский JWT): `[{id, name, host, public_key, …}]`.
- `POST /api/v1/servers` — добавить/обновить сервер в реестре (`{name, host, public_key}`); только JWT администратора. Device-токен получает `403`. Ключ обязателен, `host` — корректный `host:port`.
- `DELETE /api/v1/admin/servers/{id}` — удалить сервер из реестра (JWT).
- `POST /api/v1/auth/login` — `{ "password": "<ADMIN_PASSWORD>" }` → JWT.
- `GET /api/v1/devices` — список устройств для администратора; дополнительно содержит `ad_status`, `ad_message`, `ad_updated_at`, когда для устройства есть результат назначения.
- `DELETE /api/v1/devices/{id}` — удалить устройство из базы (JWT администратора).
- `GET /api/v1/admin/builds` — список сборок (JWT).
- `POST /api/v1/admin/builds` — загрузка сборки (JWT, `multipart/form-data`: `version`, `flavor`, `platform`, `file`) → `pending`.
- `POST /api/v1/admin/builds/{id}/approve` — подтвердить сборку со статусом `pending`; предыдущая публикация для той же платформы и flavor архивируется (JWT).
- `POST /api/v1/admin/builds/{id}/reject` — отклонить сборку со статусом `pending` (JWT).
- `DELETE /api/v1/admin/builds/{id}` — удалить запись сборки и её файл (JWT).
- `POST /api/v1/ci/builds` — загрузка из CI (Bearer `CI_UPLOAD_TOKEN` или `INVENTORY_DEVICE_TOKEN`) → `pending`.
- `GET /api/v1/downloads/rustdesk/windows/meta?flavor=normal|cashdesk` — публичный JSON для клиентского автообновления.
- `GET /api/v1/downloads/rustdesk/windows/latest?flavor=...&artifact_type=exe|msi&architecture=...` — публичное скачивание подтверждённой сборки (по умолчанию EXE, поддерживает `HEAD`).
- `GET /api/v1/install/windows.ps1?flavor=normal|cashdesk` — PowerShell-скрипт установки опубликованного MSI; перед запуском сверяется SHA-256.
