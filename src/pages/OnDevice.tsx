import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../ipc";
import { useStore } from "../store";
import type { CachePathInfo, LocalModel } from "../types";
import { formatBytes, ORIGIN_LABELS, formatDate } from "../util";
import {
  IconEye,
  IconFolder,
  IconMove,
  IconRefresh,
  IconTrash,
} from "../components/icons";

function ConfirmModal({
  title,
  body,
  confirmText = "确认",
  danger = false,
  onConfirm,
  onCancel,
}: {
  title: string;
  body: string;
  confirmText?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        <p>{body}</p>
        <div className="modal-actions">
          <button className="btn btn-ghost" onClick={onCancel}>
            取消
          </button>
          <button
            className={`btn ${danger ? "btn-danger" : "btn-primary"}`}
            onClick={onConfirm}
          >
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}

export function OnDevice() {
  const { toast, config, reloadConfig } = useStore();
  const [models, setModels] = useState<LocalModel[] | null>(null);
  const [cachePaths, setCachePaths] = useState<CachePathInfo[]>([]);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState("");
  const [origin, setOrigin] = useState<string>("all");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirmDelete, setConfirmDelete] = useState<null | { paths: string[] }>(null);
  const [busy, setBusy] = useState(false);

  const scan = useCallback(async () => {
    setModels(null);
    try {
      const [list, paths] = await Promise.all([
        api.listLocalModels(),
        api.getCachePaths(),
      ]);
      setModels(list);
      setCachePaths(paths);
      api.dirSizes(paths.filter((p) => p.exists).map((p) => p.path)).then(setSizes);
    } catch (e) {
      toast(`扫描失败：${String(e)}`, "error");
      setModels([]);
    }
  }, [toast]);

  useEffect(() => {
    if (config && models === null) scan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  const originsPresent = useMemo(() => {
    if (!models) return [];
    return [...new Set(models.map((m) => m.origin))];
  }, [models]);

  const filtered = useMemo(() => {
    if (!models) return [];
    let list = models;
    if (origin !== "all") list = list.filter((m) => m.origin === origin);
    if (filter.trim()) {
      const q = filter.trim().toLowerCase();
      list = list.filter(
        (m) =>
          m.name.toLowerCase().includes(q) ||
          m.fileName.toLowerCase().includes(q) ||
          m.family.toLowerCase().includes(q) ||
          (m.repo ?? "").toLowerCase().includes(q)
      );
    }
    return list;
  }, [models, origin, filter]);

  const groups = useMemo(() => {
    const g = new Map<string, LocalModel[]>();
    for (const m of filtered) {
      const arr = g.get(m.family) ?? [];
      arr.push(m);
      g.set(m.family, arr);
    }
    return [...g.entries()];
  }, [filtered]);

  const toggleSelect = (id: string) => {
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };

  const selectedPaths = () =>
    (models ?? []).filter((m) => selected.has(m.id)).map((m) => m.filePath);

  const doDelete = async (paths: string[]) => {
    setBusy(true);
    try {
      const res = await api.deleteLocalModels(paths);
      const okCount = res.filter((r) => r.ok).length;
      toast(`已删除 ${okCount}/${paths.length} 项（移入废纸篓）`, okCount === paths.length ? "success" : "error");
      setSelected(new Set());
      setConfirmDelete(null);
      await scan();
    } finally {
      setBusy(false);
    }
  };

  const doMove = async (paths: string[]) => {
    const dest = await api.pickFolder();
    if (!dest) return;
    setBusy(true);
    try {
      const res = await api.moveLocalModels(paths, dest);
      const ok = res.filter((r) => r.ok).length;
      toast(`已移动 ${ok}/${paths.length} 项到 ${dest}`, ok === paths.length ? "success" : "error");
      setSelected(new Set());
      await Promise.all([scan(), reloadConfig()]);
    } finally {
      setBusy(false);
    }
  };

  const totalSize = (models ?? []).reduce((a, m) => a + m.size, 0);

  return (
    <div className="page-inner">
      <div style={{ marginBottom: 18, display: "flex", alignItems: "flex-end" }}>
        <div style={{ flex: 1 }}>
          <div className="page-title">本地模型</div>
          <div className="page-subtitle">
            汇总 LalaLM 库、Hugging Face 缓存、LM Studio、ModelScope 及自定义路径中的模型
            {models ? ` · ${models.length} 个文件 · 共 ${formatBytes(totalSize)}` : ""}
          </div>
        </div>
        <button className="btn btn-ghost btn-sm" onClick={scan} disabled={busy}>
          <IconRefresh /> 重新扫描
        </button>
      </div>

      {/* storage overview — only show paths that exist on disk */}
      <div className="storage-grid">
        {cachePaths
          .filter((p) => p.exists)
          .map((p) => (
            <div key={p.label + p.path} className="card storage-card">
              <div className="sc-label">
                <span>{p.label}</span>
                {p.scanned ? (
                  <span className="badge badge-green">扫描完成</span>
                ) : (
                  <span className="badge">未启用</span>
                )}
              </div>
              <div className="sc-size">{formatBytes(sizes[p.path], 1)}</div>
              <div className="sc-path">{p.path}</div>
            </div>
          ))}
      </div>

      {/* toolbar */}
      <div className="toolbar" style={{ margin: "20px 0 4px" }}>
        <input
          placeholder="筛选本机模型…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{ width: 260 }}
        />
        <button className={`chip${origin === "all" ? " on" : ""}`} onClick={() => setOrigin("all")}>
          全部来源
        </button>
        {originsPresent.map((o) => (
          <button key={o} className={`chip${origin === o ? " on" : ""}`} onClick={() => setOrigin(o)}>
            {ORIGIN_LABELS[o] ?? o}
          </button>
        ))}
      </div>

      {/* model list */}
      {models === null ? (
        <>
          <div className="skeleton" style={{ height: 120, marginTop: 14 }} />
          <div className="skeleton" style={{ height: 120, marginTop: 12 }} />
        </>
      ) : models.length === 0 ? (
        <div className="empty-state">
          <div className="big">📦</div>
          <h3>本机还没有模型</h3>
          <p>去「发现模型」下载第一个 GGUF 模型，或在设置里添加自定义搜索路径。</p>
        </div>
      ) : (
        groups.map(([family, items]) => (
          <div key={family}>
            <div className="group-head">
              <h4>{family || "未分组"}</h4>
              <span className="count">
                {items.length} 个文件 · {formatBytes(items.reduce((a, m) => a + m.size, 0))}
              </span>
            </div>
            <div className="card">
              {items.map((m) => (
                <div key={m.id} className="local-row">
                  <input
                    type="checkbox"
                    className="checkbox"
                    checked={selected.has(m.id)}
                    onChange={() => toggleSelect(m.id)}
                  />
                  <div className="local-file">
                    <div className="local-fname" title={m.filePath}>
                      {m.fileName}
                    </div>
                    <div className="local-meta">
                      <span>{formatBytes(m.size)}</span>
                      <span>{formatDate(m.modified)}</span>
                      {m.meta?.contextLength && <span>ctx {m.meta.contextLength}</span>}
                      {m.repo && <span>{m.repo}</span>}
                    </div>
                  </div>
                  {m.quant && <span className="badge badge-cyan">{m.quant}</span>}
                  <span className="badge">{ORIGIN_LABELS[m.origin] ?? m.origin}</span>
                  <div className="row-actions">
                    <button
                      className="btn-icon"
                      title="在 Finder 中显示"
                      onClick={() =>
                        api.revealPath(m.filePath).catch((e) => toast(String(e), "error"))
                      }
                    >
                      <IconEye />
                    </button>
                    <button
                      className="btn-icon"
                      title="打开所在文件夹"
                      onClick={() => {
                        const dir = m.filePath.replace(/\/[^/]+$/, "") || "/";
                        api.openPath(dir).catch((e) => toast(String(e), "error"));
                      }}
                    >
                      <IconFolder />
                    </button>
                    <button
                      className="btn-icon"
                      title="移动到…"
                      disabled={busy}
                      onClick={() => doMove([m.filePath])}
                    >
                      <IconMove />
                    </button>
                    <button
                      className="btn-icon"
                      title="删除（移入废纸篓）"
                      disabled={busy}
                      onClick={() => setConfirmDelete({ paths: [m.filePath] })}
                    >
                      <IconTrash />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ))
      )}

      {/* bulk bar */}
      {selected.size > 0 && (
        <div className="bulkbar">
          <b>已选 {selected.size} 项</b>
          <div style={{ flex: 1 }} />
          <button className="btn btn-ghost btn-sm" onClick={() => doMove(selectedPaths())} disabled={busy}>
            <IconMove size={14} /> 移动到…
          </button>
          <button
            className="btn btn-danger btn-sm"
            onClick={() => setConfirmDelete({ paths: selectedPaths() })}
            disabled={busy}
          >
            <IconTrash size={14} /> 删除所选
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => setSelected(new Set())}>
            取消选择
          </button>
        </div>
      )}

      {confirmDelete && (
        <ConfirmModal
          title="删除模型文件？"
          body={`将把 ${confirmDelete.paths.length} 个文件移入系统废纸篓：\n${confirmDelete.paths
            .map((p) => p.split("/").pop())
            .join("、")}`}
          confirmText="删除"
          danger
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => doDelete(confirmDelete.paths)}
        />
      )}
    </div>
  );
}
