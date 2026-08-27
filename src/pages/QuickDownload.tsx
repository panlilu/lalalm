import { useCallback, useEffect, useState } from "react";
import { api, SOURCE_LABELS } from "../ipc";
import { useStore } from "../store";
import type {
  Config,
  ModelDetail,
  QuickFileStatus,
  Source,
} from "../types";
import { formatBytes, parseQuickLink } from "../util";
import { IconSearch } from "../components/icons";

type Parsed = { source: Source; repo: string; path: string | null };
type StatusMap = Record<string, QuickFileStatus>;

function StatusChip({ st }: { st?: QuickFileStatus }) {
  if (!st) return null;
  switch (st.status) {
    case "exists":
      return <span className="badge badge-green">已存在</span>;
    case "downloading":
      return <span className="badge badge-accent">下载中</span>;
    case "partial":
      return (
        <span
          className="badge badge-warn"
          title={`已有 ${formatBytes(st.onDisk)}，将断点续传`}
        >
          部分 · 可续传
        </span>
      );
    default:
      return null;
  }
}

export function QuickDownload() {
  const { toast, goDownloads, quickLink, setQuickLink } = useStore();
  const [input, setInput] = useState("");
  const [parsed, setParsed] = useState<Parsed | null>(null);
  const [detail, setDetail] = useState<ModelDetail | null>(null);
  const [statuses, setStatuses] = useState<StatusMap>({});
  const [targetDir, setTargetDir] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [includeAux, setIncludeAux] = useState(true);
  const [busy, setBusy] = useState(false);

  const analyze = useCallback(
    async (raw: string, forced?: Parsed) => {
      const p = forced ?? parseQuickLink(raw.trim());
      if (!p) {
        toast("无法识别链接：请粘贴 huggingface.co / hf-mirror.com / modelscope.cn 的模型链接", "error");
        return;
      }
      setParsed(p);
      setDetail(null);
      setSelected(new Set());
      try {
        const dir = await api.quickTargetDir(p.repo);
        setTargetDir(dir);
      } catch {
        /* ignore */
      }

      // 文件直链 → 直接进下载流程
      if (p.path) {
        const size = await api
          .getModelDetail(p.source, p.repo)
          .then((d) => d.files.find((f) => f.path === p.path)?.size ?? 0)
          .catch(() => 0);
        const st = await api
          .checkQuickFiles(p.source, p.repo, [{ path: p.path, size }])
          .catch(() => [] as QuickFileStatus[]);
        const one = st[0];
        if (one?.status === "exists") {
          toast("该文件已存在于目标目录", "success");
          return;
        }
        if (one?.status === "downloading") {
          toast("该文件正在下载中", "success");
          goDownloads();
          return;
        }
        await startBatch(p, [p.path]);
        return;
      }

      // 项目链接 → 拉取文件清单供选择
      const d = await api.getModelDetail(p.source, p.repo).catch((e) => {
        toast(`获取仓库失败：${String(e)}`, "error");
        return null;
      });
      if (!d) return;
      setDetail(d);
      const weights = d.variants.flatMap((v) => v.files.map((f) => f.path));
      try {
        const files = [
          ...d.variants.flatMap((v) => v.files),
          ...d.variants.flatMap((v) => v.companions).filter((c) => c.role !== "mmproj"),
        ];
        const sts = await api.checkQuickFiles(p.source, p.repo, files);
        const map: StatusMap = {};
        for (const st of sts) map[st.path] = st;
        setStatuses(map);
      } catch {
        /* ignore */
      }
      // 默认勾选推荐量化
      const rec =
        d.variants.find((v) => v.quant?.includes("Q4")) ??
        d.variants.find((v) => v.recommended) ??
        d.variants[0];
      if (rec && weights.length > 0) {
        setSelected(new Set(rec.files.map((f) => f.path)));
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  );

  // 从详情页/推荐卡跳转过来的预填
  useEffect(() => {
    if (quickLink) {
      setInput(linkOf(quickLink));
      analyze(linkOf(quickLink), quickLink);
      setQuickLink(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [quickLink]);

  const startBatch = async (p: Parsed, paths: string[]) => {
    setBusy(true);
    try {
      const n = await api.startDownloadBatch(p.source, p.repo, paths);
      toast(`已加入下载（${n} 个文件）→ 目标 ${targetDir || p.repo}`, "success");
      goDownloads();
    } catch (e) {
      toast(`失败：${String(e)}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const downloadSelected = async () => {
    if (!parsed) return;
    const paths = [...selected];
    if (paths.length === 0) {
      toast("未选择任何文件", "error");
      return;
    }
    await startBatch(parsed, paths);
  };

  const variantPresets = () => {
    if (!detail) return [];
    const seen = new Set<string>();
    const presets: Array<{ label: string; paths: string[] }> = [];
    for (const v of detail.variants) {
      const key = v.label;
      if (seen.has(key)) continue;
      seen.add(key);
      presets.push({ label: v.label, paths: v.files.map((f) => f.path) });
    }
    presets.push({ label: "完整仓库", paths: detail.files.map((f) => f.path) });
    return presets.slice(0, 8);
  };

  const auxFiles = detail
    ? detail.variants[0]?.companions ?? []
    : [];

  return (
    <div className="page-inner">
      <div style={{ marginBottom: 18 }}>
        <div className="page-title">快速下载</div>
        <div className="page-subtitle">
          粘贴 Hugging Face / hf-mirror / ModelScope 的模型或文件链接
        </div>
      </div>

      <div className="card" style={{ padding: 16 }}>
        <div className="search-box">
          <IconSearch size={18} />
          <input
            style={{ fontSize: 13.5 }}
            placeholder="https://hf-mirror.com/unsloth/Qwen3.8-27B-GGUF 或 …/resolve/main/xxx.gguf"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && analyze(input)}
          />
          <button
            className="btn btn-primary search-btn"
            onClick={() => analyze(input)}
            disabled={busy}
          >
            解析
          </button>
        </div>
        {parsed && (
          <div className="toolbar" style={{ marginTop: 10 }}>
            <span className="badge badge-accent">{SOURCE_LABELS[parsed.source]}</span>
            <span className="badge">{parsed.repo}</span>
            {parsed.path && <span className="badge badge-cyan">单文件直链</span>}
          </div>
        )}
      </div>

      {parsed && !parsed.path && (
        <>
          {detail && (
            <>
              {/* 快速选择 */}
              <div className="toolbar" style={{ marginTop: 16, flexWrap: "wrap" }}>
                <b style={{ fontSize: 13 }}>快速选择：</b>
                {variantPresets().map((p) => (
                  <button
                    key={p.label}
                    className="chip"
                    onClick={() => setSelected(new Set(p.paths))}
                  >
                    {p.label}（{p.paths.length} 文件）
                  </button>
                ))}
                <button
                  className="chip"
                  onClick={() =>
                    setSelected(new Set(detail.files.map((f) => f.path)))
                  }
                >
                  全选权重+配置
                </button>
              </div>

              {/* 目标目录 */}
              <div className="form-row" style={{ margin: "12px 0" }}>
                <label>保存到</label>
                <code className="path-item" style={{ flex: 1 }}>
                  {targetDir}
                </code>
              </div>

              {/* 文件列表 */}
              <div className="card" style={{ padding: "6px 14px" }}>
                {detail.variants.map((v) =>
                  v.files.map((f) => {
                    const st = statuses[f.path];
                    const done = st?.status === "exists";
                    const on = selected.has(f.path);
                    return (
                      <label
                        key={f.path}
                        className="toolbar"
                        style={{
                          padding: "7px 2px",
                          borderBottom: "1px solid var(--border-soft)",
                          cursor: done ? "default" : "pointer",
                          opacity: done ? 0.55 : 1,
                        }}
                      >
                        <input
                          type="checkbox"
                          className="checkbox"
                          disabled={done}
                          checked={done || on}
                          onChange={(e) => {
                            const next = new Set(selected);
                            if (e.target.checked) next.add(f.path);
                            else next.delete(f.path);
                            setSelected(next);
                          }}
                        />
                        <span className="file-path" style={{ flex: 1 }} title={f.path}>
                          {f.path}
                        </span>
                        <StatusChip st={st} />
                        <span style={{ fontVariantNumeric: "tabular-nums" }}>
                          {formatBytes(f.size)}
                        </span>
                      </label>
                    );
                  })
                )}

                {/* 配置 / 分词器 */}
                {auxFiles.length > 0 && (
                  <label className="toolbar" style={{ padding: "9px 2px" }}>
                    <input
                      type="checkbox"
                      className="checkbox"
                      checked={includeAux}
                      onChange={(e) => setIncludeAux(e.target.checked)}
                    />
                    <span style={{ flex: 1 }}>
                      配置与分词器等小文件 ×{auxFiles.length}
                    </span>
                    <span style={{ color: "var(--faint)" }}>
                      {formatBytes(
                        auxFiles.reduce((a, b) => a + b.size, 0)
                      )}
                    </span>
                  </label>
                )}
              </div>

              <div className="toolbar" style={{ marginTop: 14 }}>
                <button
                  className="btn btn-primary"
                  disabled={busy || selected.size === 0}
                  onClick={downloadSelected}
                >
                  开始下载（{selected.size + (includeAux ? auxFiles.length : 0)} 个文件）
                </button>
              </div>
            </>
          )}
          {!detail && <div className="skeleton" style={{ height: 200, marginTop: 16 }} />}
        </>
      )}
    </div>
  );
}

function linkOf(l: { source: Config["source"]; repo: string }): string {
  const host =
    l.source === "modelScope"
      ? "https://modelscope.cn/models/"
      : l.source === "huggingFace"
        ? "https://huggingface.co/"
        : "https://hf-mirror.com/";
  return `${host}${l.repo}`;
}
