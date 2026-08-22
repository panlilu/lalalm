import { useStore } from "../store";
import { formatBytes } from "../util";
import {
  IconCompass,
  IconDevice,
  IconDownload,
  IconGear,
} from "./icons";
import type { SysStats } from "../types";

function Meter({
  label,
  valueText,
  frac,
  hot = false,
}: {
  label: string;
  valueText: string;
  frac: number;
  hot?: boolean;
}) {
  return (
    <div className="sysmon-row">
      <label>
        <span>{label}</span>
        <b>{valueText}</b>
      </label>
      <div className={`meter${hot ? " hot" : ""}`}>
        <div style={{ width: `${Math.min(100, Math.max(2, frac * 100))}%` }} />
      </div>
    </div>
  );
}

function SysMon({ stats }: { stats?: SysStats }) {
  if (!stats) {
    return (
      <div className="sysmon">
        <div className="spinner" style={{ margin: "8px auto" }} />
      </div>
    );
  }
  const vramLabel =
    stats.vramUnified && !stats.vramTotal
      ? "统一内存"
      : stats.vramTotal
        ? formatBytes(stats.vramTotal)
        : "—";
  const gpuName = (stats.gpuName ?? "GPU")
    // Shorten vendor prefixes so long names fit the sidebar row.
    .replace(/^NVIDIA\s+GeForce\s+/i, "")
    .replace(/^NVIDIA\s+/i, "")
    .replace(/^AMD\s+(Radeon(\(TM\))?\s*)/i, "")
    .replace(/^Intel(R)?\s+/i, "");
  return (
    <div className="sysmon">
      <div className="sysmon-title">
        <span>系统状态</span>
        <span>
          {stats.arch === "aarch64" ? "Apple Silicon" : stats.arch}
        </span>
      </div>
      <Meter
        label="CPU"
        valueText={`${stats.cpuUsage.toFixed(0)}%`}
        frac={stats.cpuUsage / 100}
        hot={stats.cpuUsage > 85}
      />
      <Meter
        label={`内存 · ${formatBytes(stats.memTotal, 0)}`}
        valueText={`${formatBytes(stats.memUsed, 0)} (${stats.memPercent.toFixed(0)}%)`}
        frac={stats.memPercent / 100}
        hot={stats.memPercent > 90}
      />
      <Meter
        label={`${gpuName}${stats.vramUnified ? " · 统一内存" : " 显存"}`}
        valueText={vramLabel}
        frac={
          stats.vramTotal && stats.memTotal
            ? Math.min(1, stats.vramTotal / stats.memTotal)
            : stats.vramUnified
              ? 1
              : 0
        }
      />
      <Meter
        label={`磁盘可用 · 共 ${formatBytes(stats.diskTotal, 0)}`}
        valueText={formatBytes(stats.diskFree)}
        frac={
          stats.diskTotal ? Math.min(1, stats.diskFree / stats.diskTotal) : 0
        }
      />
    </div>
  );
}

export function Sidebar() {
  const { route, goDiscover, goDevice, goDownloads, goSettings, activeCount, sysStats } =
    useStore();

  const itemClass = (page: string) =>
    `nav-item${"page" in route && route.page === page ? " active" : ""}`;

  return (
    <aside className="sidebar">
      <div className="logo-row">
        <div className="logo-mark">L</div>
        <div>
          <div className="logo-name">LalaLM</div>
          <div className="logo-sub">本地大模型管理器</div>
        </div>
      </div>

      <nav className="nav-group">
        <button className={itemClass("discover")} onClick={goDiscover}>
          <IconCompass />
          发现模型
        </button>
        <button className={itemClass("device")} onClick={goDevice}>
          <IconDevice />
          本地模型
        </button>
        <button className={itemClass("downloads")} onClick={goDownloads}>
          <IconDownload />
          下载任务
          {activeCount > 0 && <span className="nav-badge">{activeCount}</span>}
        </button>
        <button className={itemClass("settings")} onClick={goSettings}>
          <IconGear />
          设置
        </button>
      </nav>

      <div className="sidebar-footer">
        <SysMon stats={sysStats} />
      </div>
    </aside>
  );
}
