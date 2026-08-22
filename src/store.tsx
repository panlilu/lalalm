// Global app store: navigation, live events, toasts, config.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import type { ReactNode } from "react";
import { api } from "./ipc";
import type { Config, DownloadTask, SysStats } from "./types";

export type Route =
  | { page: "discover" }
  | { page: "detail"; repo: string; source: Config["source"] }
  | { page: "device" }
  | { page: "downloads" }
  | { page: "settings" };

interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

interface Store {
  route: Route;
  goDiscover: () => void;
  goDevice: () => void;
  goDownloads: () => void;
  goSettings: () => void;
  openDetail: (repo: string, source: Config["source"]) => void;
  goBack: () => void;
  canBack: boolean;

  config?: Config;
  reloadConfig: () => Promise<void>;

  downloads: DownloadTask[];
  activeCount: number;

  sysStats?: SysStats;

  toast: (text: string, kind?: Toast["kind"]) => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const v = useContext(Ctx);
  if (!v) throw new Error("useStore outside provider");
  return v;
}

let toastSeq = 1;

export function StoreProvider({ children }: { children: ReactNode }) {
  const [route, setRoute] = useState<Route>({ page: "discover" });
  const [history, setHistory] = useState<Route[]>([]);
  const [config, setConfig] = useState<Config | undefined>();
  const [downloads, setDownloads] = useState<DownloadTask[]>([]);
  const [sysStats, setSysStats] = useState<SysStats | undefined>();
  const [toasts, setToasts] = useState<Toast[]>([]);
  const mounted = useRef(false);
  const navRef = useRef<(r: Route) => void>(() => {});

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = toastSeq++;
    setToasts((t) => [...t, { id, kind, text }]);
    window.setTimeout(() => {
      setToasts((t) => t.filter((x) => x.id !== id));
    }, 4000);
  }, []);

  const reloadConfig = useCallback(async () => {
    try {
      const c = await api.getConfig();
      setConfig(c);
    } catch (e) {
      console.error("load config failed", e);
    }
  }, []);

  // Theme: "system" tracks the OS appearance live; dark/light are forced.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const t = config?.theme ?? "system";
      const light = t === "light" || (t === "system" && !mq.matches);
      document.documentElement.classList.toggle("light", light);
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [config?.theme]);

  useEffect(() => {
    if (mounted.current) return;
    mounted.current = true;
    reloadConfig();

    const un1 = listen<DownloadTask[]>("downloads-changed", (e) =>
      setDownloads(e.payload ?? [])
    );
    const un2 = listen<SysStats>("sys-stats", (e) => setSysStats(e.payload));
    // Tray menu deep-links (e.g. 下载任务 → downloads page).
    const un3 = listen<string>("navigate", (e) => {
      const page = e.payload;
      if (page === "downloads") navRef.current({ page: "downloads" });
    });
    api.listDownloads().then(setDownloads).catch(() => {});
    api.getSysStats().then(setSysStats).catch(() => {});

    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
      un3.then((f) => f());
    };
  }, [reloadConfig]);

  const nav = useCallback(
    (r: Route) => {
      setHistory((h) => [...h, route]);
      setRoute(r);
    },
    [route]
  );
  // Tray menu events fire before/after renders; always call the latest nav.
  navRef.current = nav;

  const store: Store = {
    route,
    goDiscover: () => nav({ page: "discover" }),
    goDevice: () => nav({ page: "device" }),
    goDownloads: () => nav({ page: "downloads" }),
    goSettings: () => nav({ page: "settings" }),
    openDetail: (repo, source) => nav({ page: "detail", repo, source }),
    goBack: () => {
      setHistory((h) => {
        if (h.length === 0) return h;
        setRoute(h[h.length - 1]);
        return h.slice(0, -1);
      });
    },
    canBack: history.length > 0,
    config,
    reloadConfig,
    downloads,
    activeCount: downloads.filter(
      (d) => d.status === "active" || d.status === "queued" || d.status === "paused"
    ).length,
    sysStats,
    toast,
  };

  return (
    <Ctx.Provider value={store}>
      {children}
      <div className="toast-stack">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast-${t.kind}`}>
            {t.text}
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}
