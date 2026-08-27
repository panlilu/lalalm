import { useLayoutEffect, useRef } from "react";
import { api } from "./ipc";
import { StoreProvider, useStore } from "./store";
import { Sidebar } from "./components/Sidebar";
import { Discover } from "./pages/Discover";
import { ModelDetail } from "./pages/ModelDetail";
import { OnDevice } from "./pages/OnDevice";
import { Downloads } from "./pages/Downloads";
import { Settings } from "./pages/Settings";
import { QuickDownload } from "./pages/QuickDownload";

function CurrentPage() {
  const { route } = useStore();
  switch (route.page) {
    case "discover":
      return <Discover />;
    case "detail":
      return <ModelDetail repo={route.repo} source={route.source} />;
    case "device":
      return <OnDevice />;
    case "downloads":
      return <Downloads />;
    case "quick":
      return <QuickDownload />;
    case "settings":
      return <Settings />;
  }
}

function UpdateBanner() {
  const { updateInfo, dismissUpdate, toast } = useStore();
  if (!updateInfo) return null;
  const start = async () => {
    if (!updateInfo.assetUrl || !updateInfo.assetName) {
      window.open(updateInfo.notesUrl, "_blank");
      return;
    }
    try {
      await api.downloadDirect(updateInfo.assetUrl, updateInfo.assetName);
      toast("安装包已加入下载，完成后在「下载任务」里打开", "success");
      dismissUpdate();
    } catch (e) {
      toast(`下载失败：${String(e)}`, "error");
    }
  };
  return (
    <div
      className="toolbar"
      style={{
        padding: "8px 18px",
        background: "var(--panel-2)",
        borderBottom: "1px solid var(--border-soft)",
        fontSize: 12.5,
      }}
    >
      <span className="badge badge-accent">新版本 v{updateInfo.latest} 可用</span>
      <span style={{ color: "var(--muted)" }}>
        当前 v{updateInfo.current}
      </span>
      <div style={{ flex: 1 }} />
      <button className="btn btn-primary btn-sm" onClick={start}>
        一键更新{updateInfo.assetName ? ` · ${updateInfo.assetName}` : ""}
      </button>
      <button
        className="btn btn-ghost btn-sm"
        onClick={() => window.open(updateInfo.notesUrl, "_blank")}
      >
        查看说明
      </button>
      <button className="btn-icon" title="本次忽略" onClick={dismissUpdate}>
        ✕
      </button>
    </div>
  );
}

function Shell() {
  const { route } = useStore();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // Remember the scroll offset of every page (incl. per-repo detail pages)
  // so switching tabs restores exactly where the user left off.
  const posMap = useRef<Map<string, number>>(new Map());
  const pageKey =
    route.page === "detail"
      ? `detail:${route.source}:${route.repo}`
      : route.page;
  const keyRef = useRef(pageKey);
  keyRef.current = pageKey;

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = posMap.current.get(keyRef.current) ?? 0;
  }, [pageKey]);

  return (
    <div className="app-shell">
      <Sidebar />
      <main className="main-area">
        <UpdateBanner />
        <div
          ref={scrollRef}
          className="page"
          onScroll={(e) => {
            const el = e.currentTarget;
            posMap.current.set(keyRef.current, el.scrollTop);
          }}
        >
          <CurrentPage />
        </div>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}
