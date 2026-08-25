import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../ipc";
import { useStore } from "../store";
import type { DlStatus, DownloadTask } from "../types";
import { formatBytes, formatEta, formatSpeed, percent } from "../util";
import {
  IconCheck,
  IconEye,
  IconPause,
  IconPlay,
  IconRetry,
  IconTrash,
  IconX,
} from "../components/icons";

const STATUS_META: Record<DlStatus, { label: string; cls: string }> = {
  queued: { label: "排队中", cls: "badge" },
  active: { label: "下载中", cls: "badge badge-accent" },
  paused: { label: "已暂停", cls: "badge badge-warn" },
  completed: { label: "已完成", cls: "badge badge-green" },
  error: { label: "失败", cls: "badge badge-red" },
  cancelled: { label: "已取消", cls: "badge" },
  interrupted: { label: "已中断", cls: "badge badge-warn" },
};

function TaskCard({ t }: { t: DownloadTask }) {
  const { toast } = useStore();
  const pct = percent(t.downloaded, t.total);
  // Redirect targets sometimes omit Content-Length: total stays 0 while
  // bytes flow. Show an indeterminate bar instead of a stuck 0%.
  const unknownSize = t.total <= 0 && t.downloaded > 0;
  const eta = t.speed > 0 ? (t.total - t.downloaded) / t.speed : Infinity;
  const act = (p: Promise<unknown>, okMsg?: string) =>
    p
      .then(() => okMsg && toast(okMsg))
      .catch((e) => toast(String(e), "error"));

  return (
    <div className="card dl-card">
      <div className="dl-top">
        <span className={`dl-name`} title={`${t.repo}/${t.path}`}>
          {t.out}
          <span style={{ color: "var(--faint)", fontWeight: 400 }}> · {t.repo}</span>
        </span>
        <span className={STATUS_META[t.status].cls}>{STATUS_META[t.status].label}</span>
        <div className="dl-actions">
          {(t.status === "active" || t.status === "queued") && (
            <button
              className="btn-icon"
              title="暂停"
              onClick={() => act(api.pauseDownload(t.id), "已暂停")}
            >
              <IconPause />
            </button>
          )}
          {t.status === "paused" && (
            <button
              className="btn-icon"
              title="继续"
              onClick={() => act(api.resumeDownload(t.id), "已继续")}
            >
              <IconPlay />
            </button>
          )}
          {["error", "cancelled", "interrupted"].includes(t.status) && (
            <>
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => act(api.retryDownload(t.id), "重试中（断点续传）…")}
              >
                <IconRetry size={13} /> 重试
              </button>
              <button
                className="btn-icon"
                title="移除记录"
                onClick={() => act(api.removeDownload(t.id))}
              >
                <IconTrash size={15} />
              </button>
            </>
          )}
          {["active", "paused", "queued"].includes(t.status) && (
            <button
              className="btn-icon"
              title="取消下载"
              onClick={() => act(api.cancelDownload(t.id), "已取消，可稍后重试续传")}
            >
              <IconX size={16} />
            </button>
          )}
          {t.status === "completed" && (
            <>
              <button
                className="btn-icon"
                title="在 Finder 中显示"
                onClick={() =>
                  act(api.revealPath(`${t.dir}/${t.out}`))
                }
              >
                <IconEye />
              </button>
              <button
                className="btn-icon"
                title="移除记录"
                onClick={() => act(api.removeDownload(t.id))}
              >
                <IconTrash size={15} />
              </button>
            </>
          )}
        </div>
      </div>

      <div
        className={`progress${t.status === "completed" ? " done" : ""}${
          t.status === "error" ? " error" : ""
        }${unknownSize ? " indeterminate" : ""}`}
      >
        <div
          style={{
            width:
              unknownSize && t.status === "active"
                ? "100%"
                : `${t.status === "completed" ? 100 : pct}%`,
          }}
        />
      </div>

      <div className="dl-sub">
        <span>{unknownSize ? (t.status === "active" ? "下载中…" : "—") : `${pct.toFixed(1)}%`}</span>
        <span>
          {formatBytes(t.downloaded)} / {formatBytes(t.total)}
        </span>
        {t.status === "active" && <span>⚡ {formatSpeed(t.speed)}</span>}
        {t.status === "active" && isFinite(eta) && <span>剩余 {formatEta(eta)}</span>}
        {t.error && (
          <span style={{ color: "var(--danger)" }} title={t.error}>
            {t.error.length > 60 ? `${t.error.slice(0, 60)}…` : t.error}
          </span>
        )}
        <span style={{ marginLeft: "auto" }}>
          {new Date(t.updatedAt * 1000).toLocaleTimeString("zh-CN")}
        </span>
      </div>
    </div>
  );
}

export function Downloads() {
  const { downloads, goDiscover, toast } = useStore();
  const [logOpen, setLogOpen] = useState(false);
  const [logText, setLogText] = useState("");
  const logRef = useRef<HTMLPreElement | null>(null);

  const refreshLog = useCallback(async () => {
    try {
      const t = await api.readAria2Log(200);
      setLogText(t);
    } catch (e) {
      setLogText(`读取日志失败: ${String(e)}`);
    }
  }, []);

  useEffect(() => {
    if (!logOpen) return;
    refreshLog();
    const iv = window.setInterval(refreshLog, 2000);
    return () => window.clearInterval(iv);
  }, [logOpen, refreshLog]);

  useEffect(() => {
    if (logOpen && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logText, logOpen]);

  const active = downloads.filter((d) =>
    ["active", "queued", "paused"].includes(d.status)
  );
  const finished = downloads.filter(
    (d) => !["active", "queued", "paused"].includes(d.status)
  );

  const totalSpeed = active.reduce((a, d) => a + d.speed, 0);

  return (
    <div className="page-inner">
      <div style={{ marginBottom: 18, display: "flex", alignItems: "flex-end" }}>
        <div style={{ flex: 1 }}>
          <div className="page-title">下载任务</div>
          <div className="page-subtitle">
            aria2c 多线程加速 · 支持暂停 / 继续 / 取消 / 断点续传
          </div>
        </div>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => setLogOpen((v) => !v)}
        >
          {logOpen ? "收起运行日志 ▲" : "aria2 运行日志 ▼"}
        </button>
        {active.length > 0 && (
          <span className="badge badge-accent" style={{ fontSize: 12.5, padding: "5px 12px" }}>
            总速度 {formatSpeed(totalSpeed)}
          </span>
        )}
        {finished.length > 0 && (
          <button
            className="btn btn-ghost btn-sm"
            style={{ marginLeft: 10 }}
            onClick={() => api.clearFinishedDownloads().catch(() => {})}
          >
            <IconCheck size={14} /> 清除已完成记录
          </button>
        )}
      </div>

      {logOpen && (
        <div className="card" style={{ marginBottom: 16, padding: 0, overflow: "hidden" }}>
          <div
            className="toolbar"
            style={{
              padding: "9px 14px",
              justifyContent: "space-between",
              borderBottom: "1px solid var(--border-soft)",
            }}
          >
            <b style={{ fontSize: 12.5 }}>aria2c 运行日志（每 2 秒自动刷新）</b>
            <div className="toolbar" style={{ gap: 8 }}>
              <button className="btn btn-ghost btn-sm" onClick={refreshLog}>
                刷新
              </button>
              {downloads[0] && (
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => {
                    const d = downloads.find((x) => ["active", "queued", "paused"].includes(x.status)) ?? downloads[0];
                    api.revealPath(d.dir).catch((e) => toast(String(e), "error"));
                  }}
                >
                  打开下载目录
                </button>
              )}
            </div>
          </div>
          <pre
            ref={logRef}
            style={{
              margin: 0,
              padding: "12px 14px",
              maxHeight: 260,
              overflow: "auto",
              fontSize: 11.5,
              lineHeight: 1.5,
              background: "var(--bg)",
              color: "var(--muted)",
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
              userSelect: "text",
            }}
          >
            {logText || "加载中…"}
          </pre>
        </div>
      )}

      {downloads.length === 0 ? (
        <div className="empty-state">
          <div className="big">🚀</div>
          <h3>暂无下载任务</h3>
          <p>到「发现模型」找一个喜欢的模型开始下载吧。</p>
          <button className="btn btn-primary" style={{ marginTop: 14 }} onClick={goDiscover}>
            去发现模型
          </button>
        </div>
      ) : (
        <>
          {active.length > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {active.map((t) => (
                <TaskCard key={t.id} t={t} />
              ))}
            </div>
          )}

          {finished.length > 0 && (
            <>
              <div className="divider" />
              <h4 style={{ fontSize: 13.5, marginBottom: 10 }}>历史记录</h4>
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                {finished.slice(0, 50).map((t) => (
                  <TaskCard key={t.id} t={t} />
                ))}
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
