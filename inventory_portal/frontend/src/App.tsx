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

type Build = {
  id: number;
  flavor: string;
  platform: string;
  version: string;
  file_name: string;
  file_size: number;
  sha256: string;
  status: string;
  uploaded_at: string;
  approved_at?: string | null;
  approved_by: string;
};

const TOKEN_KEY = "inv_portal_jwt";

const FLAVORS = ["normal", "cashdesk"];
const PLATFORMS = ["windows", "linux", "macos", "android"];

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

function statusLabel(status: string): string {
  switch (status) {
    case "published":
      return "Опубликована";
    case "pending":
      return "Ожидает";
    case "rejected":
      return "Отклонена";
    case "archived":
      return "Архив";
    default:
      return status;
  }
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

function IconUpload() {
  return (
    <svg className="fluent-btn-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <path d="M12 16V4m0 0l4 4m-4-4L8 8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M4 14v4a2 2 0 002 2h12a2 2 0 002-2v-4" strokeLinecap="round" />
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

  const [builds, setBuilds] = useState<Build[]>([]);
  const [buildsErr, setBuildsErr] = useState("");
  const [buildsMsg, setBuildsMsg] = useState("");
  const [buildVersion, setBuildVersion] = useState("");
  const [buildFlavor, setBuildFlavor] = useState("normal");
  const [buildPlatform, setBuildPlatform] = useState("windows");
  const [buildUploading, setBuildUploading] = useState(false);
  const [selectedBuildName, setSelectedBuildName] = useState("");
  const [deletingId, setDeletingId] = useState("");
  const buildFileInputRef = useRef<HTMLInputElement>(null);

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setDevices([]);
    setBuilds([]);
    setBuildsErr("");
    setBuildsMsg("");
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

  const deleteDevice = useCallback(
    async (id: string) => {
      if (!token) return;
      if (!window.confirm(`Удалить устройство ${id} из списка?`)) return;
      setListErr("");
      setDeletingId(id);
      try {
        const r = await fetch(`${apiBase()}/api/v1/devices/${encodeURIComponent(id)}`, {
          method: "DELETE",
          headers: { Authorization: `Bearer ${token}` },
        });
        if (r.status === 401) {
          logout();
          return;
        }
        if (r.status === 404) {
          setListErr("Устройство уже удалено");
          await load();
          return;
        }
        if (!r.ok) {
          setListErr("Не удалось удалить устройство");
          return;
        }
        setDevices((prev) => prev.filter((d) => d.rustdesk_id !== id));
      } catch {
        setListErr("Ошибка сети при удалении");
      } finally {
        setDeletingId("");
      }
    },
    [token, logout, load]
  );

  const loadBuilds = useCallback(async () => {
    if (!token) return;
    setBuildsErr("");
    try {
      const r = await fetch(`${apiBase()}/api/v1/admin/builds`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (r.status === 401) {
        logout();
        return;
      }
      if (!r.ok) {
        setBuildsErr("Не удалось получить список сборок");
        return;
      }
      setBuilds((await r.json()) as Build[]);
    } catch {
      setBuildsErr("Ошибка сети при загрузке сборок");
    }
  }, [token, logout]);

  useEffect(() => {
    if (!token) return;
    void load();
    void loadBuilds();
  }, [token, load, loadBuilds]);

  const uploadBuild = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file || !token) return;
      setBuildsErr("");
      setBuildsMsg("");
      const ver = buildVersion.trim();
      if (!ver) {
        setBuildsErr("Укажите версию сборки (например 1.4.9)");
        e.target.value = "";
        return;
      }
      setSelectedBuildName(file.name);
      const form = new FormData();
      form.append("version", ver);
      form.append("flavor", buildFlavor);
      form.append("platform", buildPlatform);
      form.append("file", file);
      setBuildUploading(true);
      try {
        const r = await fetch(`${apiBase()}/api/v1/admin/builds`, {
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
            setBuildsErr("Файл слишком большой для загрузки");
          } else {
            setBuildsErr(errText || "Не удалось загрузить сборку");
          }
          return;
        }
        setBuildsMsg("Сборка загружена и ожидает подтверждения администратора.");
        await loadBuilds();
      } catch {
        setBuildsErr("Ошибка сети при загрузке файла");
      } finally {
        setBuildUploading(false);
        e.target.value = "";
      }
    },
    [token, logout, buildVersion, buildFlavor, buildPlatform, loadBuilds]
  );

  const approveBuild = useCallback(
    async (id: number) => {
      if (!token) return;
      setBuildsErr("");
      setBuildsMsg("");
      try {
        const r = await fetch(`${apiBase()}/api/v1/admin/builds/${id}/approve`, {
          method: "POST",
          headers: { Authorization: `Bearer ${token}` },
        });
        if (r.status === 401) {
          logout();
          return;
        }
        if (!r.ok) {
          setBuildsErr("Не удалось подтвердить сборку");
          return;
        }
        setBuildsMsg("Сборка подтверждена и будет раздаваться клиентам.");
        await loadBuilds();
      } catch {
        setBuildsErr("Ошибка сети");
      }
    },
    [token, logout, loadBuilds]
  );

  const rejectBuild = useCallback(
    async (id: number) => {
      if (!token) return;
      setBuildsErr("");
      setBuildsMsg("");
      try {
        const r = await fetch(`${apiBase()}/api/v1/admin/builds/${id}/reject`, {
          method: "POST",
          headers: { Authorization: `Bearer ${token}` },
        });
        if (r.status === 401) {
          logout();
          return;
        }
        if (!r.ok) {
          setBuildsErr("Не удалось отклонить сборку");
          return;
        }
        setBuildsMsg("Сборка отклонена.");
        await loadBuilds();
      } catch {
        setBuildsErr("Ошибка сети");
      }
    },
    [token, logout, loadBuilds]
  );

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
                <h2>Сборки и обновления</h2>
                <p>
                  Загрузите новую сборку и укажите <span className="mono">flavor</span> (<span className="mono">normal</span>{" "}
                  или <span className="mono">cashdesk</span>) и версию. Сборка получит статус «Ожидает» и будет раздаваться
                  клиентам <strong>только после подтверждения</strong>. Автоматически её может залить и CI
                  (<span className="mono">POST /api/v1/ci/builds</span>).
                </p>
              </div>
              <span className="fluent-badge" title="Всего сборок">
                {builds.length}
              </span>
            </div>

            <div className="fluent-upload-grid">
              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label">Версия</span>
                <input
                  className="fluent-text-input mono"
                  type="text"
                  value={buildVersion}
                  onChange={(e) => setBuildVersion(e.target.value)}
                  placeholder="Например 1.4.9"
                  disabled={buildUploading}
                  autoComplete="off"
                />
              </div>
              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label">Flavor</span>
                <select
                  className="fluent-text-input"
                  value={buildFlavor}
                  onChange={(e) => setBuildFlavor(e.target.value)}
                  disabled={buildUploading}
                >
                  {FLAVORS.map((f) => (
                    <option key={f} value={f}>
                      {f}
                    </option>
                  ))}
                </select>
              </div>
              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label">Платформа</span>
                <select
                  className="fluent-text-input"
                  value={buildPlatform}
                  onChange={(e) => setBuildPlatform(e.target.value)}
                  disabled={buildUploading}
                >
                  {PLATFORMS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
              </div>
              <div className="fluent-upload-field">
                <span className="fluent-upload-field-label">Файл</span>
                <div className="fluent-upload-row">
                  <input
                    ref={buildFileInputRef}
                    className="fluent-file-input"
                    type="file"
                    accept=".exe,.msi,.deb,.rpm,.zst,.apk,.dmg,.pkg,application/octet-stream"
                    onChange={uploadBuild}
                    disabled={buildUploading}
                  />
                  <button
                    type="button"
                    className="fluent-btn fluent-btn-secondary fluent-upload-pick"
                    disabled={buildUploading}
                    onClick={() => buildFileInputRef.current?.click()}
                  >
                    <IconUpload />
                    {buildUploading ? "Загрузка…" : "Выбрать файл"}
                  </button>
                  <div className="fluent-file-name-plate mono" title={selectedBuildName || undefined}>
                    {selectedBuildName || "Файл не выбран"}
                  </div>
                </div>
              </div>
            </div>

            {buildsErr ? <div className="fluent-error">{buildsErr}</div> : null}
            {buildsMsg ? <div className="fluent-success">{buildsMsg}</div> : null}

            {builds.length === 0 ? (
              <div className="fluent-empty">Сборок пока нет.</div>
            ) : (
              <div className="fluent-table-wrap">
                <table className="fluent-table">
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>Flavor</th>
                      <th>Платформа</th>
                      <th>Версия</th>
                      <th>Файл</th>
                      <th>Размер</th>
                      <th>Статус</th>
                      <th>Загружена</th>
                      <th>Действия</th>
                    </tr>
                  </thead>
                  <tbody>
                    {builds.map((b) => (
                      <tr key={b.id}>
                        <td className="mono">{b.id}</td>
                        <td className="mono">{b.flavor}</td>
                        <td className="mono">{b.platform}</td>
                        <td className="mono">{b.version}</td>
                        <td className="mono" title={b.sha256 ? `sha256: ${b.sha256}` : undefined}>
                          {b.file_name}
                        </td>
                        <td>{formatBytes(b.file_size)}</td>
                        <td>{statusLabel(b.status)}</td>
                        <td className="mono">{b.uploaded_at}</td>
                        <td>
                          {b.status === "pending" ? (
                            <div className="fluent-btn-group">
                              <button
                                type="button"
                                className="fluent-btn fluent-btn-primary"
                                onClick={() => void approveBuild(b.id)}
                              >
                                Подтвердить
                              </button>
                              <button
                                type="button"
                                className="fluent-btn fluent-btn-secondary"
                                onClick={() => void rejectBuild(b.id)}
                              >
                                Отклонить
                              </button>
                            </div>
                          ) : (
                            "—"
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
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
                  void loadBuilds();
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
                      <th aria-label="Действия" />
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
                        <td>
                          <button
                            type="button"
                            className="fluent-btn fluent-btn-secondary fluent-row-delete"
                            title="Удалить устройство"
                            aria-label={`Удалить устройство ${d.rustdesk_id}`}
                            disabled={deletingId === d.rustdesk_id}
                            onClick={() => void deleteDevice(d.rustdesk_id)}
                          >
                            {deletingId === d.rustdesk_id ? "…" : "Удалить"}
                          </button>
                        </td>
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
