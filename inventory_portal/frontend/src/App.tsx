import { useCallback, useEffect, useMemo, useRef, useState } from "react";

type Device = {
  rustdesk_id: string;
  hostname: string;
  os_info: string;
  username: string;
  ip_public: string;
  ip_local: string;
  temporary_password: string;
  computer_summary: string;
  app_version: string;
  updated_at: string;
};

type DownloadAsset = {
  available: boolean;
  file_name: string | null;
  file_size: number | null;
  uploaded_at: string | null;
  download_path: string;
  published_version?: string | null;
};

const TOKEN_KEY = "inv_portal_jwt";

function apiBase(): string {
  return import.meta.env.PROD ? "" : "";
}

function formatBytes(value: number | null): string {
  if (!value || value <= 0) return "—";
  const units = ["Б", "КБ", "МБ", "ГБ"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${size >= 10 || unitIndex === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unitIndex]}`;
}

function absoluteDownloadUrl(path: string | null, available: boolean): string {
  if (!path || !available || typeof window === "undefined") return "";
  return new URL(path, window.location.origin).toString();
}

function IconMonitor() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" aria-hidden>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8M12 16v4" strokeLinecap="round" />
    </svg>
  );
}

function IconSearch() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-3-3" strokeLinecap="round" />
    </svg>
  );
}

/** Иконки по мотивам ответа MCP Magic (lucide-подобная геометрия), без новой зависимости */
function IconUpload() {
  return (
    <svg className="fluent-btn-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <path d="M12 16V4m0 0l4 4m-4-4L8 8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M4 14v4a2 2 0 002 2h12a2 2 0 002-2v-4" strokeLinecap="round" />
    </svg>
  );
}

function IconCopy() {
  return (
    <svg className="fluent-btn-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" strokeLinecap="round" />
    </svg>
  );
}

function IconExternalLink() {
  return (
    <svg className="fluent-btn-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <path d="M18 13v6a2 2 0 01-2 2H6a2 2 0 01-2-2V8a2 2 0 012-2h6" strokeLinecap="round" />
      <path d="M15 3h6v6M10 14L21 3" strokeLinecap="round" />
    </svg>
  );
}

export default function App() {
  const [token, setToken] = useState<string | null>(() =>
    typeof localStorage !== "undefined" ? localStorage.getItem(TOKEN_KEY) : null
  );
  const [password, setPassword] = useState("");
  const [loginErr, setLoginErr] = useState("");
  const [loading, setLoading] = useState(false);
  const [devices, setDevices] = useState<Device[]>([]);
  const [listErr, setListErr] = useState("");
  const [q, setQ] = useState("");
  const [downloadAsset, setDownloadAsset] = useState<DownloadAsset | null>(null);
  const [downloadErr, setDownloadErr] = useState("");
  const [downloadMsg, setDownloadMsg] = useState("");
  const [uploading, setUploading] = useState(false);
  const [selectedUploadName, setSelectedUploadName] = useState("");
  const [releaseVersion, setReleaseVersion] = useState("");
  const rustdeskFileInputRef = useRef<HTMLInputElement>(null);

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setDevices([]);
    setDownloadAsset(null);
    setDownloadErr("");
    setDownloadMsg("");
  }, []);

  const login = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoginErr("");
    setLoading(true);
    try {
      const r = await fetch(`${apiBase()}/api/v1/auth/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password }),
      });
      if (!r.ok) {
        setLoginErr("Неверный пароль или ошибка сервера");
        setLoading(false);
        return;
      }
      const j = await r.json();
      localStorage.setItem(TOKEN_KEY, j.token);
      setToken(j.token);
      setPassword("");
    } catch {
      setLoginErr("Сеть недоступна");
    }
    setLoading(false);
  };

  const loadDownloadAsset = useCallback(async () => {
    if (!token) return;
    setDownloadErr("");
    try {
      const r = await fetch(`${apiBase()}/api/v1/admin/downloads/rustdesk`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (r.status === 401) {
        logout();
        return;
      }
      if (!r.ok) {
        setDownloadErr("Не удалось получить данные по файлу RustDesk");
        return;
      }
      const asset = (await r.json()) as DownloadAsset;
      setDownloadAsset(asset);
      if (asset.published_version) {
        setReleaseVersion(asset.published_version);
      }
    } catch {
      setDownloadErr("Ошибка сети при загрузке данных файла");
    }
  }, [token, logout]);

  const load = useCallback(async () => {
    if (!token) return;
    setListErr("");
    try {
      const r = await fetch(`${apiBase()}/api/v1/devices`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (r.status === 401) {
        logout();
        return;
      }
      if (!r.ok) {
        setListErr("Не удалось загрузить список");
        return;
      }
      setDevices(await r.json());
    } catch {
      setListErr("Ошибка сети");
    }
  }, [token, logout]);

  useEffect(() => {
    if (!token) return;
    void load();
    void loadDownloadAsset();
  }, [token, load, loadDownloadAsset]);

  const uploadBinary = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file || !token) return;

      setDownloadErr("");
      setDownloadMsg("");

      if (!file.name.toLowerCase().endsWith(".exe")) {
        setSelectedUploadName("");
        setDownloadErr("Можно загружать только файлы .exe");
        e.target.value = "";
        return;
      }

      const ver = releaseVersion.trim();
      if (!ver) {
        setDownloadErr("Укажите версию сборки (например 1.4.7) — она нужна для автообновления клиентов");
        e.target.value = "";
        return;
      }

      setSelectedUploadName(file.name);
      const form = new FormData();
      form.append("version", ver);
      form.append("file", file);
      setUploading(true);

      try {
        const r = await fetch(`${apiBase()}/api/v1/admin/downloads/rustdesk`, {
          method: "POST",
          headers: { Authorization: `Bearer ${token}` },
          body: form,
        });
        if (r.status === 401) {
          logout();
          return;
        }
        if (!r.ok) {
          const errText = await r.text();
          if (r.status === 413) {
            setDownloadErr("Файл слишком большой для загрузки");
          } else if (errText === "only .exe files are allowed") {
            setDownloadErr("Можно загружать только файлы .exe");
          } else if (errText === "version is required") {
            setDownloadErr("Укажите версию сборки");
          } else {
            setDownloadErr("Не удалось загрузить rustdesk.exe");
          }
          return;
        }
        const nextAsset = (await r.json()) as DownloadAsset;
        setDownloadAsset(nextAsset);
        setDownloadMsg("Файл загружен. Ссылка для скачивания готова.");
      } catch {
        setDownloadErr("Ошибка сети при загрузке файла");
      } finally {
        setUploading(false);
        e.target.value = "";
      }
    },
    [token, logout, releaseVersion]
  );

  const downloadUrl = useMemo(
    () => absoluteDownloadUrl(downloadAsset?.download_path ?? null, Boolean(downloadAsset?.available)),
    [downloadAsset]
  );

  const copyDownloadUrl = useCallback(async () => {
    if (!downloadUrl) return;
    setDownloadErr("");
    setDownloadMsg("");
    try {
      await navigator.clipboard.writeText(downloadUrl);
      setDownloadMsg("Ссылка скопирована.");
    } catch {
      setDownloadErr("Не удалось скопировать ссылку");
    }
  }, [downloadUrl]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return devices;
    return devices.filter(
      (d) =>
        d.rustdesk_id.includes(s) ||
        d.hostname.toLowerCase().includes(s) ||
        d.username.toLowerCase().includes(s) ||
        d.ip_public.includes(s) ||
        d.os_info.toLowerCase().includes(s)
    );
  }, [devices, q]);

  if (!token) {
    return (
      <div className="win11-login">
        <div className="win11-login-card">
          <div style={{ display: "flex", alignItems: "center", gap: 14, marginBottom: 8 }}>
            <div className="win11-nav-logo">R</div>
            <div>
              <div style={{ fontWeight: 600, fontSize: 15 }}>RustDesk</div>
              <div style={{ fontSize: 12, color: "var(--win-text-tertiary)" }}>Портал учёта устройств</div>
            </div>
          </div>
          <h1>Вход</h1>
          <p className="lead">Введите пароль администратора.</p>
          <form onSubmit={login}>
            <label htmlFor="pw">Пароль</label>
            <input
              id="pw"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button type="submit" className="fluent-btn fluent-btn-primary" disabled={loading || !password}>
              {loading ? "Вход…" : "Войти"}
            </button>
            {loginErr ? <p className="fluent-error" style={{ marginTop: 16, marginBottom: 0 }}>{loginErr}</p> : null}
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="win11-app">
      <div className="win11-titlebar">RustDesk — учёт устройств</div>
      <div className="win11-body">
        <aside className="win11-nav" aria-label="Навигация">
          <div className="win11-nav-brand">
            <div className="win11-nav-logo">R</div>
            <div>
              <div className="win11-nav-title">RustDesk</div>
              <div className="win11-nav-sub">Инвентаризация</div>
            </div>
          </div>
          <div className="win11-nav-item">
            <IconMonitor />
            Устройства
          </div>
        </aside>
        <main className="win11-main">
          <h1 className="win11-page-title">Устройства</h1>
          <p className="win11-page-desc">
            Список клиентов, отправивших отчёт. Данные обновляются по расписанию с клиента.
          </p>

          <div className="fluent-card fluent-upload-card">
            <div className="fluent-upload-header">
              <div>
                <h2>Раздача RustDesk</h2>
                <p>
                  Загрузите актуальный <span className="mono">rustdesk.exe</span>, укажите номер версии — клиенты RustDesk с
                  включённой проверкой обновлений и тем же URL портала, что и для отчётов, смогут подтянуть эту сборку
                  автоматически (только Windows, <span className="mono">.exe</span>). Ссылку ниже можно по-прежнему
                  раздавать для ручной загрузки.
                </p>
              </div>
              <span className="fluent-badge" title="Статус файла">
                {downloadAsset?.available ? "Готово" : "Нет файла"}
              </span>
            </div>

            <div className="fluent-upload-version-block">
              <label className="fluent-upload-field-label" htmlFor="rustdesk-release-version">
                Версия этой сборки
              </label>
              <input
                id="rustdesk-release-version"
                className="fluent-text-input mono fluent-upload-version-input"
                type="text"
                value={releaseVersion}
                onChange={(e) => setReleaseVersion(e.target.value)}
                placeholder="Например 1.4.7"
                disabled={uploading}
                autoComplete="off"
              />
              <p className="fluent-upload-version-hint">
                Должна быть <strong>выше</strong>, чем версия в собранном клиенте, иначе обновление не предложится.
              </p>
            </div>

            <div className="fluent-upload-grid">
              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label" id="rustdesk-upload-label">
                  Новый файл
                </span>
                <div className="fluent-upload-row" role="group" aria-labelledby="rustdesk-upload-label">
                  <input
                    ref={rustdeskFileInputRef}
                    id="rustdesk-upload"
                    className="fluent-file-input"
                    type="file"
                    accept=".exe,application/octet-stream"
                    onChange={uploadBinary}
                    disabled={uploading}
                    aria-label="Выбор файла rustdesk.exe"
                  />
                  <button
                    type="button"
                    className="fluent-btn fluent-btn-secondary fluent-upload-pick"
                    disabled={uploading}
                    onClick={() => rustdeskFileInputRef.current?.click()}
                  >
                    <IconUpload />
                    {uploading ? "Загрузка…" : "Выбрать файл"}
                  </button>
                  <div className="fluent-file-name-plate mono" title={selectedUploadName || undefined}>
                    {selectedUploadName || "Файл не выбран"}
                  </div>
                </div>
              </div>

              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label" id="rustdesk-link-label">
                  Публичная ссылка
                </span>
                <div className="fluent-upload-row" role="group" aria-labelledby="rustdesk-link-label">
                  <input
                    id="rustdesk-download-link"
                    className="fluent-text-input mono fluent-upload-link-input"
                    type="text"
                    readOnly
                    value={downloadUrl}
                    placeholder="Загрузите файл, чтобы получить ссылку"
                  />
                  <button
                    type="button"
                    className="fluent-btn fluent-btn-secondary fluent-upload-action"
                    onClick={() => void copyDownloadUrl()}
                    disabled={!downloadUrl}
                    title="Копировать ссылку"
                  >
                    <IconCopy />
                    <span className="fluent-upload-action-text">Копировать</span>
                  </button>
                  <a
                    className={`fluent-btn fluent-btn-secondary fluent-upload-action${downloadUrl ? "" : " is-disabled"}`}
                    href={downloadUrl || undefined}
                    target="_blank"
                    rel="noreferrer"
                    aria-disabled={!downloadUrl}
                    title="Открыть ссылку"
                  >
                    <IconExternalLink />
                    <span className="fluent-upload-action-text">Открыть</span>
                  </a>
                </div>
              </div>

              <div className="fluent-upload-meta fluent-upload-meta-full">
                <span>Версия на портале: {downloadAsset?.published_version ?? "—"}</span>
                <span>Файл: {downloadAsset?.file_name ?? "—"}</span>
                <span>Размер: {formatBytes(downloadAsset?.file_size ?? null)}</span>
                <span>Обновлён: {downloadAsset?.uploaded_at ?? "—"}</span>
              </div>
            </div>

            {downloadErr ? <div className="fluent-error">{downloadErr}</div> : null}
            {downloadMsg ? <div className="fluent-success">{downloadMsg}</div> : null}
          </div>

          <div className="win11-commandbar">
            <div className="fluent-search-wrap">
              <IconSearch />
              <input
                className="fluent-search"
                type="search"
                placeholder="Поиск по ID, имени ПК, пользователю, IP…"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                aria-label="Поиск"
              />
            </div>
            <div className="fluent-btn-group">
              <span className="fluent-badge" title="Записей в списке">
                {filtered.length}
              </span>
              <button
                type="button"
                className="fluent-btn fluent-btn-secondary"
                onClick={() => {
                  void load();
                  void loadDownloadAsset();
                }}
              >
                Обновить
              </button>
              <button type="button" className="fluent-btn fluent-btn-secondary" onClick={logout}>
                Выйти
              </button>
            </div>
          </div>

          {listErr ? <div className="fluent-error">{listErr}</div> : null}

          <div className="fluent-card">
            {filtered.length === 0 ? (
              <div className="fluent-empty">Нет записей или ничего не найдено по запросу.</div>
            ) : (
              <div className="fluent-table-wrap">
                <table className="fluent-table">
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>Компьютер</th>
                      <th>Пользователь</th>
                      <th>ОС</th>
                      <th>IP (внешн.)</th>
                      <th>IP (лок.)</th>
                      <th>Врем. пароль</th>
                      <th>Версия</th>
                      <th>Обновлено</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map((d) => (
                      <tr key={d.rustdesk_id}>
                        <td className="mono">{d.rustdesk_id}</td>
                        <td>{d.hostname || "—"}</td>
                        <td>{d.username || "—"}</td>
                        <td className="mono" title={d.computer_summary}>
                          {d.os_info.length > 36 ? `${d.os_info.slice(0, 36)}…` : d.os_info}
                        </td>
                        <td className="mono">{d.ip_public || "—"}</td>
                        <td className="mono">{d.ip_local || "—"}</td>
                        <td className="mono">{d.temporary_password || "—"}</td>
                        <td>{d.app_version || "—"}</td>
                        <td className="mono">{d.updated_at}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
