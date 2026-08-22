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
        }`}
      >
        <div style={{ width: `${t.status === "completed" ? 100 : pct}%` }} />
      </div>

      <div className="dl-sub">
        <span>{pct.toFixed(1)}%</span>
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
  const { downloads, goDiscover } = useStore();
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
