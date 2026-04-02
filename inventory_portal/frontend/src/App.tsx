import { useCallback, useEffect, useMemo, useState } from "react";

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

const TOKEN_KEY = "inv_portal_jwt";

function apiBase(): string {
  return import.meta.env.PROD ? "" : "";
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

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setDevices([]);
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

  useEffect(() => {
    if (token) void load();
  }, [token, load]);

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
      <div className="login-wrap">
        <div className="card">
          <h2>Учёт RustDesk</h2>
          <p>Войдите паролем администратора (как в переменной окружения сервера).</p>
          <form onSubmit={login}>
            <div className="field">
              <label htmlFor="pw">Пароль</label>
              <input
                id="pw"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
            <button type="submit" disabled={loading || !password}>
              {loading ? "Вход…" : "Войти"}
            </button>
            {loginErr ? <div className="error">{loginErr}</div> : null}
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="layout">
      <header className="header">
        <h1>RustDesk — устройства</h1>
        <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
          <span className="badge">{filtered.length}</span>
          <button type="button" className="secondary" onClick={() => void load()}>
            Обновить
          </button>
          <button type="button" className="secondary" onClick={logout}>
            Выйти
          </button>
        </div>
      </header>
      <main className="main">
        <div className="toolbar">
          <input
            type="search"
            placeholder="Поиск: ID, хост, пользователь, IP…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        {listErr ? <div className="error">{listErr}</div> : null}
        {filtered.length === 0 ? (
          <div className="empty">Нет записей или ничего не найдено.</div>
        ) : (
          <div className="table-wrap">
            <table>
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
                      {d.os_info.length > 40 ? `${d.os_info.slice(0, 40)}…` : d.os_info}
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
      </main>
    </div>
  );
}
