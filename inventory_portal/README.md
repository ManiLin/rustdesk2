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

## Раздача `rustdesk.exe`

- После входа в админку можно загрузить актуальный `rustdesk.exe` и **обязательно указать версию** (например `1.4.7`). Версия должна быть **выше**, чем `VERSION` в собранном клиенте — иначе автообновление не предложит установку.
- Портал покажет готовую публичную ссылку. Её можно отправлять пользователям: при открытии начнётся скачивание файла.
- Публичный JSON для проверки обновлений (без авторизации): `GET /api/v1/downloads/rustdesk/windows/meta` — поля `available`, `version`, `download_path`.
- Публичный путь скачивания: `GET /api/v1/downloads/rustdesk/windows/latest`
- Файл хранится на сервере в каталоге `UPLOAD_DIR` (по умолчанию `/data/downloads`).
- Лимит размера на API задаётся переменной `MAX_UPLOAD_BYTES` (по умолчанию `536870912`, то есть 512 МБ).
- У фронтового nginx в образе задан `client_max_body_size 512m`, иначе загрузка обрывалась бы ответом 413 ещё до бэкенда.
- У Axum для `Multipart` по умолчанию лимит тела запроса около **2 МБ**; на маршруте загрузки включён `DefaultBodyLimit` до значения `MAX_UPLOAD_BYTES`, иначе большой `rustdesk.exe` не доходит до обработчика (в UI было бы общее «не удалось загрузить»).

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
- `POST /api/v1/auth/login` — `{ "password": "<ADMIN_PASSWORD>" }` → JWT.
- `GET /api/v1/devices` — заголовок `Authorization: Bearer <jwt>`.
- `GET /api/v1/admin/downloads/rustdesk` — заголовок `Authorization: Bearer <jwt>`, статус загруженного файла.
- `POST /api/v1/admin/downloads/rustdesk` — заголовок `Authorization: Bearer <jwt>`, `multipart/form-data`: поля **`version`** (строка, обязательно) и **`file`** (`.exe`).
- `GET /api/v1/downloads/rustdesk/windows/meta` — публичный JSON для клиентского автообновления.
- `GET /api/v1/downloads/rustdesk/windows/latest` — публичная ссылка на скачивание `rustdesk.exe`.
