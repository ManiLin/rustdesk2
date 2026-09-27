import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type React from "react";

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
  ad_status?: string;
  ad_message?: string;
  ad_updated_at?: string;
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

type TagServer = {
  id: number;
  name: string;
  host: string;
  public_key: string;
  created_by: string;
  created_at: string;
  updated_at: string;
};

type Page = "devices" | "builds" | "servers";
type ApiFailure = { code?: string; message?: string };
type Confirmation = { title: string; message: string; actionLabel: string; run: () => Promise<void> };

const TOKEN_KEY = "inv_portal_jwt";
const FLAVORS = ["normal", "cashdesk"];
const PLATFORMS = ["windows", "linux", "macos", "android"];
const PAGE_INFO: Record<Page, { title: string; description: string }> = {
  devices: {
    title: "Устройства",
    description: "Компьютеры, которые отправляют отчёты в портал.",
  },
  builds: {
    title: "Сборки",
    description: "Загружайте сборки и публикуйте их после проверки.",
  },
  servers: {
    title: "Серверы RustDesk",
    description: "Общий список серверов, доступный клиентам RustDesk.",
  },
};

function apiErrorMessage(body: string, fallback: string): string {
  try {
    const parsed = JSON.parse(body) as ApiFailure;
    const knownMessages: Record<string, string> = {
      "bad password": "Неверный пароль администратора.",
      unauthorized: "Сессия завершена. Войдите в портал заново.",
      "administrator access required": "Это действие доступно только администратору.",
      "invalid server host, use host:port": "Введите адрес сервера в формате host:port.",
      "public key is required": "Укажите публичный ключ сервера.",
      "another server already uses this host": "Этот адрес уже занят другим сервером.",
      "only pending builds can be approved": "Подтвердить можно только сборку, ожидающую проверки.",
      "only pending builds can be rejected": "Отклонить можно только сборку, ожидающую проверки.",
      "build is no longer pending": "Статус сборки уже изменился. Обновите список.",
      "file is too large": "Файл превышает допустимый размер.",
      "file is required": "Выберите файл сборки.",
      "device not found": "Устройство уже удалено или не найдено.",
      "server not found": "Сервер уже удалён или не найден.",
      "build not found": "Сборка не найдена.",
      "unsupported platform": "Выберите поддерживаемую платформу.",
      "flavor must be normal or cashdesk": "Выберите канал normal или cashdesk.",
      "platform must be windows, linux, macos, or android": "Выберите поддерживаемую платформу.",
      "unsupported file extension for platform": "Формат файла не подходит для выбранной платформы.",
      "version is required": "Укажите версию сборки.",
      "flavor is required": "Выберите канал сборки.",
      "platform is required": "Выберите платформу сборки.",
      "sha256 mismatch": "Контрольная сумма файла не совпала.",
      "db error": "Не удалось сохранить данные. Повторите попытку позже.",
    };
    if (parsed.message) return knownMessages[parsed.message] ?? parsed.message;
  } catch {
    // Older portal versions returned plain text errors.
  }
  return body.trim() || fallback;
}

async function responseError(response: Response, fallback: string): Promise<string> {
  return apiErrorMessage(await response.text(), fallback);
}

function formatBytes(value: number): string {
  if (!value || value <= 0) return "—";
  const units = ["Б", "КБ", "МБ", "ГБ"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size >= 10 || unit === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
}

function formatDate(value?: string): string {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("ru-RU");
}

function formatStatus(status?: string): { label: string; kind: string } {
  switch (status) {
    case "published": return { label: "Опубликована", kind: "success" };
    case "pending": return { label: "Ожидает проверки", kind: "warning" };
    case "rejected": return { label: "Отклонена", kind: "error" };
    case "archived": return { label: "В архиве", kind: "neutral" };
    case "assigned": return { label: "Добавлен", kind: "success" };
    case "skipped": return { label: "Пропущен", kind: "neutral" };
    case "error": return { label: "Ошибка", kind: "error" };
    default: return { label: "Не назначался", kind: "neutral" };
  }
}

function FilledButton(props: React.HTMLAttributes<HTMLElement> & { disabled?: boolean; type?: string }) {
  return <md-filled-button {...props}>{props.children}</md-filled-button>;
}

function OutlinedButton(props: React.HTMLAttributes<HTMLElement> & { disabled?: boolean; type?: string }) {
  return <md-outlined-button {...props}>{props.children}</md-outlined-button>;
}

function StatusPill({ status }: { status?: string }) {
  const value = formatStatus(status);
  return <span className={`status-pill status-${value.kind}`}>{value.label}</span>;
}

function Alert({ kind = "error", children }: { kind?: "error" | "success" | "info"; children: React.ReactNode }) {
  return <div className={`alert alert-${kind}`} role={kind === "error" ? "alert" : "status"}>{children}</div>;
}

function Loading({ label = "Загрузка…" }: { label?: string }) {
  return <div className="loading-state"><md-circular-progress indeterminate aria-label={label} />{label}</div>;
}

function NavigationIcon({ page }: { page: Page }) {
  const paths: Record<Page, React.ReactNode> = {
    devices: <><rect x="3" y="4" width="8" height="7" rx="1" /><rect x="13" y="4" width="8" height="7" rx="1" /><rect x="3" y="13" width="8" height="7" rx="1" /><rect x="13" y="13" width="8" height="7" rx="1" /></>,
    builds: <><path d="M12 16V4m0 0 4 4m-4-4L8 8" /><path d="M4 14v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4" /></>,
    servers: <><rect x="3" y="4" width="18" height="6" rx="2" /><rect x="3" y="14" width="18" height="6" rx="2" /><path d="M7 7h.01M7 17h.01M11 7h6M11 17h6" /></>,
  };
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[page]}</svg>;
}

export default function App() {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY));
  const [page, setPage] = useState<Page>("devices");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const confirmDialog = useRef<HTMLDialogElement>(null);
  const [password, setPassword] = useState("");
  const [loginError, setLoginError] = useState("");
  const [loginLoading, setLoginLoading] = useState(false);
  const [devices, setDevices] = useState<Device[]>([]);
  const [builds, setBuilds] = useState<Build[]>([]);
  const [servers, setServers] = useState<TagServer[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState("");
  const [passwordVisible, setPasswordVisible] = useState<string[]>([]);
  const [buildVersion, setBuildVersion] = useState("");
  const [buildFlavor, setBuildFlavor] = useState("normal");
  const [buildPlatform, setBuildPlatform] = useState("windows");
  const [buildFile, setBuildFile] = useState<File | null>(null);
  const [serverName, setServerName] = useState("");
  const [serverHost, setServerHost] = useState("");
  const [serverKey, setServerKey] = useState("");
  const [editingServer, setEditingServer] = useState<number | null>(null);

  useEffect(() => {
    const dialog = confirmDialog.current;
    if (!dialog) return;
    if (confirmation && !dialog.open) dialog.showModal();
    if (!confirmation && dialog.open) dialog.close();
  }, [confirmation]);

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setDevices([]);
    setBuilds([]);
    setServers([]);
  }, []);

  const authHeaders = useCallback(() => ({ Authorization: `Bearer ${token}` }), [token]);

  const loadData = useCallback(async (showSpinner = true) => {
    if (!token) return;
    if (showSpinner) setLoading(true);
    setError("");
    try {
      const requests = await Promise.all([
        fetch("/api/v1/devices", { headers: authHeaders() }),
        fetch("/api/v1/admin/builds", { headers: authHeaders() }),
        fetch("/api/v1/servers", { headers: authHeaders() }),
      ]);
      if (requests.some((response) => response.status === 401)) {
        logout();
        return;
      }
      const failed = requests.find((response) => !response.ok);
      if (failed) {
        setError(await responseError(failed, "Не удалось загрузить данные портала"));
        return;
      }
      const [deviceRows, buildRows, serverRows] = await Promise.all(requests.map((response) => response.json()));
      setDevices(deviceRows as Device[]);
      setBuilds(buildRows as Build[]);
      setServers(serverRows as TagServer[]);
    } catch {
      setError("Не удалось связаться с порталом. Проверьте подключение и повторите попытку.");
    } finally {
      if (showSpinner) setLoading(false);
    }
  }, [token, authHeaders, logout]);

  useEffect(() => {
    if (token) void loadData();
  }, [token, loadData]);

  const login = async (event: React.FormEvent) => {
    event.preventDefault();
    setLoginError("");
    setLoginLoading(true);
    try {
      const response = await fetch("/api/v1/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password }),
      });
      if (!response.ok) {
        setLoginError(await responseError(response, "Не удалось войти"));
        return;
      }
      const result = (await response.json()) as { token: string };
      localStorage.setItem(TOKEN_KEY, result.token);
      setToken(result.token);
      setPassword("");
    } catch {
      setLoginError("Не удалось связаться с порталом. Проверьте подключение.");
    } finally {
      setLoginLoading(false);
    }
  };

  const performDeleteDevice = async (device: Device) => {
    if (!token) return;
    setBusyId(device.rustdesk_id);
    setError("");
    try {
      const response = await fetch(`/api/v1/devices/${encodeURIComponent(device.rustdesk_id)}`, {
        method: "DELETE", headers: authHeaders(),
      });
      if (response.status === 401) return logout();
      if (!response.ok) throw new Error(await responseError(response, "Не удалось удалить устройство"));
      setDevices((current) => current.filter((row) => row.rustdesk_id !== device.rustdesk_id));
      setNotice("Устройство удалено");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось удалить устройство");
    } finally {
      setBusyId("");
    }
  };

  const deleteDevice = (device: Device) => setConfirmation({
    title: "Удалить устройство?",
    message: `${device.hostname || device.rustdesk_id} будет удалено из списка портала.`,
    actionLabel: "Удалить",
    run: () => performDeleteDevice(device),
  });

  const uploadBuild = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!token || !buildFile) {
      setError("Выберите файл сборки");
      return;
    }
    const form = new FormData();
    form.append("version", buildVersion.trim());
    form.append("flavor", buildFlavor);
    form.append("platform", buildPlatform);
    form.append("file", buildFile);
    setBusyId("upload");
    setError("");
    setNotice("");
    try {
      const response = await fetch("/api/v1/admin/builds", { method: "POST", headers: authHeaders(), body: form });
      if (response.status === 401) return logout();
      if (!response.ok) throw new Error(await responseError(response, "Не удалось загрузить сборку"));
      setBuildVersion("");
      setBuildFile(null);
      await loadData(false);
      setNotice("Сборка загружена и ожидает проверки администратора");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось загрузить сборку");
    } finally {
      setBusyId("");
    }
  };

  const changeBuildStatus = async (build: Build, action: "approve" | "reject") => {
    if (!token) return;
    setBusyId(`build-${build.id}`);
    setError("");
    setNotice("");
    try {
      const response = await fetch(`/api/v1/admin/builds/${build.id}/${action}`, {
        method: "POST", headers: authHeaders(),
      });
      if (response.status === 401) return logout();
      if (!response.ok) throw new Error(await responseError(response, "Не удалось изменить статус сборки"));
      await loadData(false);
      setNotice(action === "approve" ? "Сборка опубликована" : "Сборка отклонена");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось изменить статус сборки");
    } finally {
      setBusyId("");
    }
  };

  const saveServer = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!token) return;
    setBusyId("server-save");
    setError("");
    setNotice("");
    try {
      const response = await fetch("/api/v1/servers", {
        method: "POST",
        headers: { ...authHeaders(), "Content-Type": "application/json" },
        body: JSON.stringify({ name: serverName.trim(), host: serverHost.trim(), public_key: serverKey.trim() }),
      });
      if (response.status === 401) return logout();
      if (!response.ok) throw new Error(await responseError(response, "Не удалось сохранить сервер"));
      await loadData(false);
      setNotice(editingServer ? "Изменения сервера сохранены" : "Сервер добавлен в общий список");
      setEditingServer(null);
      setServerName("");
      setServerHost("");
      setServerKey("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось сохранить сервер");
    } finally {
      setBusyId("");
    }
  };

  const editServer = (server: TagServer) => {
    setEditingServer(server.id);
    setServerName(server.name);
    setServerHost(server.host);
    setServerKey(server.public_key);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  const performDeleteServer = async (server: TagServer) => {
    if (!token) return;
    setBusyId(`server-${server.id}`);
    setError("");
    try {
      const response = await fetch(`/api/v1/admin/servers/${server.id}`, { method: "DELETE", headers: authHeaders() });
      if (response.status === 401) return logout();
      if (!response.ok) throw new Error(await responseError(response, "Не удалось удалить сервер"));
      setServers((current) => current.filter((row) => row.id !== server.id));
      if (editingServer === server.id) {
        setEditingServer(null);
        setServerName("");
        setServerHost("");
        setServerKey("");
      }
      setNotice("Сервер удалён");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось удалить сервер");
    } finally {
      setBusyId("");
    }
  };

  const deleteServer = (server: TagServer) => setConfirmation({
    title: "Удалить сервер?",
    message: `${server.host} будет удалён из общего списка и перестанет отображаться у клиентов.`,
    actionLabel: "Удалить",
    run: () => performDeleteServer(server),
  });

  const filteredDevices = useMemo(() => {
    const value = query.trim().toLowerCase();
    if (!value) return devices;
    return devices.filter((device) => [
      device.rustdesk_id, device.hostname, device.username, device.ip_public, device.os_info,
    ].some((field) => field.toLowerCase().includes(value)));
  }, [devices, query]);

  if (!token) {
    return (
      <main className="login-shell">
        <form className="login-card surface-card" onSubmit={login}>
          <h1>Вход в портал</h1>
          <p className="supporting-text">Введите пароль администратора, чтобы управлять устройствами и сборками.</p>
          <md-outlined-text-field
            label="Пароль администратора"
            type="password"
            autocomplete="current-password"
            required
            value={password}
            onInput={(event: React.FormEvent<HTMLElement>) => setPassword((event.currentTarget as HTMLInputElement).value)}
          />
          {loginError && <Alert>{loginError}</Alert>}
          <FilledButton type="submit" disabled={loginLoading || !password}>
            {loginLoading ? "Вход…" : "Войти"}
          </FilledButton>
        </form>
      </main>
    );
  }

  const info = PAGE_INFO[page];
  return (
    <div className="app-shell">
      <header className="top-app-bar">
        <button className="brand-button" type="button" onClick={() => setPage("devices")} aria-label="На главную">
          <span className="brand-mark">R</span><span>RustDesk</span>
        </button>
        <span className="top-app-title">Портал учёта</span>
        <div className="top-actions">
          <OutlinedButton onClick={() => void loadData()}>Обновить</OutlinedButton>
          <md-text-button onClick={logout}>Выйти</md-text-button>
        </div>
      </header>
      <div className="app-layout">
        <nav className="navigation-rail" aria-label="Разделы портала">
          {([
            ["devices", "Устройства"],
            ["builds", "Сборки"],
            ["servers", "Серверы"],
          ] as [Page, string][]).map(([id, label]) => (
            <button
              key={id}
              className={`nav-item ${page === id ? "selected" : ""}`}
              type="button"
              aria-current={page === id ? "page" : undefined}
              onClick={() => { setPage(id); setError(""); setNotice(""); }}
            >
              <span className="nav-icon"><NavigationIcon page={id} /></span><span>{label}</span>
            </button>
          ))}
          <div className="nav-footer">Панель администратора</div>
        </nav>
        <main className="page-content">
          <div className="page-heading">
            <div><div className="eyebrow">Управление</div><h1>{info.title}</h1><p>{info.description}</p></div>
            <div className="page-count">{page === "devices" ? devices.length : page === "builds" ? builds.length : servers.length}</div>
          </div>
          {error && <Alert>{error}</Alert>}
          {notice && <Alert kind="success">{notice}</Alert>}
          {loading ? <Loading /> : (
            <>
              {page === "devices" && (
                <section className="surface-card data-card" aria-label="Список устройств">
                  <div className="toolbar">
                    <md-outlined-text-field
                      className="search-field"
                      label="Поиск по ID, компьютеру, пользователю, IP"
                      type="search"
                      value={query}
                      onInput={(event: React.FormEvent<HTMLElement>) => setQuery((event.currentTarget as HTMLInputElement).value)}
                    />
                    <span className="result-count">{filteredDevices.length} из {devices.length}</span>
                  </div>
                  {filteredDevices.length === 0 ? <div className="empty-state">{devices.length ? "Ничего не найдено. Измените запрос." : "Устройства пока не отправляли отчёты."}</div> : (
                    <div className="table-scroll"><table className="data-table">
                      <thead><tr><th>Устройство</th><th>Пользователь</th><th>Система</th><th>IP</th><th>AD → адресная книга</th><th>Временный пароль</th><th>Обновлено</th><th /></tr></thead>
                      <tbody>{filteredDevices.map((device) => (
                        <tr key={device.rustdesk_id}>
                          <td><strong>{device.hostname || "Без имени"}</strong><span className="secondary-cell mono">{device.rustdesk_id}</span></td>
                          <td>{device.username || "—"}</td>
                          <td title={device.computer_summary}>{device.os_info || "—"}<span className="secondary-cell">{device.app_version}</span></td>
                          <td className="mono">{device.ip_local || device.ip_public || "—"}</td>
                          <td><StatusPill status={device.ad_status} />{device.ad_updated_at && <span className="secondary-cell">{formatDate(device.ad_updated_at)}</span>}{device.ad_message && <span className="secondary-cell" title={device.ad_message}>{device.ad_message}</span>}</td>
                          <td><div className="password-cell"><code>{passwordVisible.includes(device.rustdesk_id) ? device.temporary_password || "—" : "••••••••"}</code><md-text-button onClick={() => setPasswordVisible((current) => current.includes(device.rustdesk_id) ? current.filter((id) => id !== device.rustdesk_id) : [...current, device.rustdesk_id])} aria-label={passwordVisible.includes(device.rustdesk_id) ? "Скрыть пароль" : "Показать пароль"}>{passwordVisible.includes(device.rustdesk_id) ? "Скрыть" : "Показать"}</md-text-button></div></td>
                          <td>{device.updated_at || "—"}</td>
                          <td><md-text-button disabled={busyId === device.rustdesk_id} onClick={() => void deleteDevice(device)}>Удалить</md-text-button></td>
                        </tr>
                      ))}</tbody>
                    </table></div>
                  )}
                </section>
              )}

              {page === "builds" && (
                <>
                  <section className="surface-card form-card">
                    <div className="section-heading"><div><h2>Загрузить сборку</h2><p>Новая сборка появится у клиентов после подтверждения.</p></div><StatusPill status="pending" /></div>
                    <form className="form-grid" onSubmit={(event) => void uploadBuild(event)}>
                      <md-outlined-text-field label="Версия" placeholder="Например, 1.4.9" required value={buildVersion} onInput={(event: React.FormEvent<HTMLElement>) => setBuildVersion((event.currentTarget as HTMLInputElement).value)} />
                      <md-outlined-select label="Канал" value={buildFlavor} onChange={(event: React.FormEvent<HTMLElement>) => setBuildFlavor((event.currentTarget as HTMLSelectElement).value)}>{FLAVORS.map((flavor) => <md-select-option key={flavor} value={flavor}><div slot="headline">{flavor}</div></md-select-option>)}</md-outlined-select>
                      <md-outlined-select label="Платформа" value={buildPlatform} onChange={(event: React.FormEvent<HTMLElement>) => setBuildPlatform((event.currentTarget as HTMLSelectElement).value)}>{PLATFORMS.map((platform) => <md-select-option key={platform} value={platform}><div slot="headline">{platform}</div></md-select-option>)}</md-outlined-select>
                      <label className="native-field file-field"><span>Файл сборки</span><input type="file" accept=".exe,.msi,.deb,.rpm,.zst,.apk,.dmg,.pkg,.appimage,.gz" onChange={(event) => setBuildFile(event.target.files?.[0] ?? null)} required /></label>
                      <div className="form-actions"><FilledButton type="submit" disabled={busyId === "upload"}>{busyId === "upload" ? "Загрузка…" : "Загрузить на проверку"}</FilledButton></div>
                    </form>
                  </section>
                  <section className="surface-card data-card">
                    <div className="section-heading"><div><h2>История сборок</h2><p>Только сборки «Ожидают проверки» можно подтвердить или отклонить.</p></div></div>
                    {builds.length === 0 ? <div className="empty-state">Сборок пока нет.</div> : <div className="table-scroll"><table className="data-table"><thead><tr><th>Сборка</th><th>Канал</th><th>Платформа</th><th>Файл и размер</th><th>Статус</th><th>Загружена</th><th>Действия</th></tr></thead><tbody>
                      {builds.map((build) => <tr key={build.id}>
                        <td><strong>{build.version}</strong><span className="secondary-cell">#{build.id}</span></td><td>{build.flavor}</td><td>{build.platform}</td><td>{build.file_name}<span className="secondary-cell">{formatBytes(build.file_size)}</span></td><td><StatusPill status={build.status} /></td><td>{build.uploaded_at}</td>
                        <td>{build.status === "pending" ? <div className="row-actions"><md-filled-button disabled={busyId === `build-${build.id}`} onClick={() => void changeBuildStatus(build, "approve")}>Опубликовать</md-filled-button><md-outlined-button disabled={busyId === `build-${build.id}`} onClick={() => void changeBuildStatus(build, "reject")}>Отклонить</md-outlined-button></div> : "—"}</td>
                      </tr>)}
                    </tbody></table></div>}
                  </section>
                </>
              )}

              {page === "servers" && (
                <>
                  <section className="surface-card form-card">
                    <div className="section-heading"><div><h2>{editingServer ? "Изменить сервер" : "Добавить сервер"}</h2><p>Изменения сразу появятся в общем списке клиентов.</p></div></div>
                    <form className="form-grid server-form" onSubmit={(event) => void saveServer(event)}>
                      <md-outlined-text-field label="Название" placeholder="Например, филиал Казань" value={serverName} onInput={(event: React.FormEvent<HTMLElement>) => setServerName((event.currentTarget as HTMLInputElement).value)} />
                      <md-outlined-text-field label="Адрес host:port" placeholder="rustdesk.example.ru:21116" required readonly={editingServer !== null} value={serverHost} onInput={(event: React.FormEvent<HTMLElement>) => setServerHost((event.currentTarget as HTMLInputElement).value)} />
                      <md-outlined-text-field className="wide-field" label="Публичный ключ" required value={serverKey} onInput={(event: React.FormEvent<HTMLElement>) => setServerKey((event.currentTarget as HTMLInputElement).value)} />
                      <div className="form-actions"><FilledButton type="submit" disabled={busyId === "server-save"}>{busyId === "server-save" ? "Сохранение…" : editingServer ? "Сохранить изменения" : "Добавить сервер"}</FilledButton>{editingServer && <md-text-button type="button" onClick={() => { setEditingServer(null); setServerName(""); setServerHost(""); setServerKey(""); }}>Отмена</md-text-button>}</div>
                    </form>
                  </section>
                  <section className="surface-card data-card">
                    <div className="section-heading"><div><h2>Общий список</h2><p>Клиенты используют эти серверы при настройке тегов адресной книги.</p></div></div>
                    {servers.length === 0 ? <div className="empty-state">Серверов пока нет.</div> : <div className="table-scroll"><table className="data-table"><thead><tr><th>Название</th><th>Адрес</th><th>Публичный ключ</th><th>Обновлён</th><th>Действия</th></tr></thead><tbody>
                      {servers.map((server) => <tr key={server.id}><td><strong>{server.name || server.host}</strong></td><td className="mono">{server.host}</td><td><code className="key-value" title={server.public_key}>{server.public_key}</code></td><td>{server.updated_at}</td><td><div className="row-actions"><md-text-button onClick={() => editServer(server)}>Изменить</md-text-button><md-text-button disabled={busyId === `server-${server.id}`} onClick={() => void deleteServer(server)}>Удалить</md-text-button></div></td></tr>)}
                    </tbody></table></div>}
                  </section>
                </>
              )}
            </>
          )}
          <footer className="page-footer">Данные портала обновляются автоматически с устройств.</footer>
          <dialog
            ref={confirmDialog}
            className="confirm-dialog"
            aria-labelledby="confirm-title"
            onCancel={(event) => { event.preventDefault(); setConfirmation(null); }}
          >
            {confirmation && <>
              <h2 id="confirm-title">{confirmation.title}</h2>
              <p>{confirmation.message}</p>
              <div className="confirm-actions">
                <md-text-button onClick={() => setConfirmation(null)}>Отмена</md-text-button>
                <md-filled-button onClick={() => {
                  const action = confirmation.run;
                  setConfirmation(null);
                  void action();
                }}>{confirmation.actionLabel}</md-filled-button>
              </div>
            </>}
          </dialog>
        </main>
      </div>
    </div>
  );
}
