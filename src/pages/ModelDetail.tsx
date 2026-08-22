import { useCallback, useEffect, useMemo, useState } from "react";
import { api, SOURCE_LABELS } from "../ipc";
import { useStore } from "../store";
import type { ModelDetail as ModelDetailData, Source, Variant } from "../types";
import {
  assessRun,
  formatBytes,
  formatCount,
  formatDate,
  formatLabel,
  repoWebUrl,
  RUN_LEVEL_CLASS,
} from "../util";
import { Markdown } from "../components/Markdown";
import {
  IconArrowDown,
  IconBack,
  IconCheck,
  IconDownload,
} from "../components/icons";

function VerdictChip({ size }: { size: number }) {
  const { sysStats } = useStore();
  const v = assessRun(size, sysStats?.memTotal);
  return (
    <span className={RUN_LEVEL_CLASS[v.level]} title={v.desc}>
      ●{v.level === "fine" || v.level === "ok" ? <IconCheck size={11} /> : null}
      {v.label}
    </span>
  );
}

const ALL_SOURCES: Source[] = ["modelScope", "huggingFace", "hfMirror"];

export function ModelDetail({
  repo,
  source,
}: {
  repo: string;
  source: ModelDetailData["summary"]["source"];
}) {
  const { goBack, toast, goDownloads, openDetail, sysStats } = useStore();
  const [data, setData] = useState<ModelDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [showAllFiles, setShowAllFiles] = useState(false);
  const [includeMmproj, setIncludeMmproj] = useState(true);
  const [starting, setStarting] = useState(false);
  // Availability of the same repo on the other hubs (for source tabs).
  const [existsOn, setExistsOn] = useState<Partial<Record<Source, boolean>>>({});
  const [avatarBroken, setAvatarBroken] = useState(false);

  useEffect(() => {
    let alive = true;
    setData(null);
    setError(null);
    setExpandedId(null);
    setAvatarBroken(false);
    api
      .getModelDetail(source, repo)
      .then((d) => alive && setData(d))
      .catch((e) => alive && setError(String(e)));
    // Probe the same repo on the other hubs in parallel.
    setExistsOn({});
    Promise.all(
      ALL_SOURCES.filter((s) => s !== source).map(async (s) => {
        const ok = await api.checkRepoExists(s, repo).catch(() => false);
        return [s, ok] as const;
      })
    ).then((entries) => {
      if (alive) setExistsOn(Object.fromEntries(entries));
    });
    return () => {
      alive = false;
    };
  }, [repo, source]);

  /** Paths to download for a variant: weights + config/tokenizer (+ mmproj opt-in). */
  const variantPaths = useCallback((v: Variant): string[] => {
    const paths = v.files.map((f) => f.path);
    for (const c of v.companions) {
      if (c.role === "mmproj") {
        if (includeMmproj) paths.push(c.path);
      } else {
        // config / tokenizer are tiny and always included
        paths.push(c.path);
      }
    }
    return paths;
  }, [includeMmproj]);

  const download = useCallback(
    async (v: Variant) => {
      setStarting(true);
      try {
        const n = await api.startDownloadBatch(source, repo, variantPaths(v));
        toast(`已加入下载：${v.label}（${n} 个文件）`, "success");
        setTimeout(() => goDownloads(), 500);
      } catch (e) {
        toast(`下载失败：${String(e)}`, "error");
      } finally {
        setStarting(false);
      }
    },
    [source, repo, variantPaths, toast, goDownloads]
  );

  const hasMmproj = useMemo(
    () => data?.variants.some((v) => v.companions.some((c) => c.role === "mmproj")) ?? false,
    [data]
  );

  if (error) {
    return (
      <div className="page-inner">
        <div className="back-row">
          <button className="btn btn-ghost btn-sm" onClick={goBack}>
            <IconBack /> 返回
          </button>
        </div>
        <div className="error-banner">
          <span>加载模型详情失败：{error}</span>
          <button className="btn btn-ghost btn-sm" onClick={() => window.location.reload()}>
            刷新
          </button>
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="page-inner">
        <div className="back-row">
          <button className="btn btn-ghost btn-sm" onClick={goBack}>
            <IconBack /> 返回
          </button>
        </div>
        <div className="skeleton" style={{ height: 160 }} />
        <div className="skeleton" style={{ height: 300, marginTop: 14 }} />
      </div>
    );
  }

  const s = data.summary;
  const recommended = data.variants.find((v) => v.recommended);

  // Cross-source availability: HF and hf-mirror always share the same index,
  // so they count as one family.
  const hfFamilyOk =
    source !== "modelScope" ||
    existsOn.huggingFace === true ||
    existsOn.hfMirror === true;
  const msOk = source === "modelScope" || existsOn.modelScope === true;
  const sourceAvail: Record<Source, boolean> = {
    huggingFace: source === "huggingFace" ? true : hfFamilyOk,
    hfMirror: source === "hfMirror" ? true : hfFamilyOk,
    modelScope: msOk,
  };

  return (
    <div className="page-inner">
      <div className="back-row">
        <button className="btn btn-ghost btn-sm" onClick={goBack}>
          <IconBack /> 返回
        </button>

        {/* cross-source switcher */}
        <div className="seg" title="同一模型在其他仓库的可用性">
          {ALL_SOURCES.map((s) => {
            const ok = sourceAvail[s];
            const cur = s === source;
            return (
              <button
                key={s}
                className={cur ? "on" : ""}
                disabled={!ok && !cur}
                style={!ok && !cur ? { opacity: 0.4, textDecoration: "line-through" } : undefined}
                title={
                  cur
                    ? `当前来源：${SOURCE_LABELS[s]}`
                    : ok
                      ? `此模型在 ${SOURCE_LABELS[s]} 上也存在，点击切换`
                      : `${SOURCE_LABELS[s]} 未收录此模型`
                }
                onClick={() => {
                  if (!cur && ok) openDetail(repo, s);
                }}
              >
                {SOURCE_LABELS[s]}
              </button>
            );
          })}
        </div>

        <span className="badge">{formatLabel(data.format)}</span>
        <div style={{ flex: 1 }} />
        <button
          className="btn btn-ghost btn-sm"
          onClick={() =>
            api.openUrl(repoWebUrl(source, repo)).catch((e) => toast(String(e), "error"))
          }
          title={`在浏览器打开 ${repoWebUrl(source, repo)}`}
        >
          ↗ 原页面
        </button>
      </div>

      {/* hero */}
      <div className="card detail-hero">
        <div className="hero-top">
          {s.avatar && !avatarBroken && (
            <img
              className="mc-avatar"
              src={s.avatar}
              alt=""
              referrerPolicy="no-referrer"
              onError={() => setAvatarBroken(true)}
              style={{ width: 52, height: 52, borderRadius: 12 }}
            />
          )}
          <div style={{ minWidth: 0, flex: 1 }}>
            <div className="hero-title">{s.name}</div>
            <div className="hero-repo">{s.repo}</div>
          </div>
        </div>

        <div style={{ display: "flex", gap: 16, color: "var(--muted)", fontSize: 12.5, flexWrap: "wrap" }}>
          <span>⬇ {formatCount(s.downloads)} 下载</span>
          <span>❤ {formatCount(s.likes)} 点赞</span>
          <span>更新于 {formatDate(s.lastModified)}</span>
          {s.params && <span>{s.params} 参数</span>}
          {sysStats && sysStats.memTotal > 0 && (
            <span title="运行判定依据本机内存估算（权重 × 1.25 预留计算开销）">
              本机内存 {formatBytes(sysStats.memTotal, 0)}
              {sysStats.vramUnified ? " · 统一内存" : ""}
            </span>
          )}
        </div>

        {recommended && (
          <div className="hero-actions">
            <button
              className="btn btn-primary"
              disabled={starting}
              onClick={() => download(recommended)}
              title={`推荐量化 ${recommended.label}，含配置文件${hasMmproj && includeMmproj ? "与视觉模块" : ""}`}
            >
              <IconDownload />
              快速下载推荐版 · {recommended.label}（{formatBytes(recommended.totalSize)}）
            </button>
          </div>
        )}
      </div>

      {/* variants */}
      <div className="card" style={{ marginTop: 14, padding: "16px 18px" }}>
        <div className="toolbar" style={{ justifyContent: "space-between", marginBottom: 12 }}>
          <b style={{ fontSize: 14 }}>
            选择量化版本
            <span style={{ color: "var(--faint)", fontWeight: 400, fontSize: 12, marginLeft: 10 }}>
              按本机内存评估可运行性 · 权重×1.25 预留计算开销
            </span>
          </b>
          {hasMmproj && (
            <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12.5, color: "var(--muted)", cursor: "pointer" }}>
              <input
                type="checkbox"
                className="checkbox"
                checked={includeMmproj}
                onChange={(e) => setIncludeMmproj(e.target.checked)}
              />
              同时下载视觉模块（mmproj，多模态需要）
            </label>
          )}
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {data.variants.map((v) => {
            const verdict = assessRun(v.totalSize, sysStats?.memTotal);
            const expanded = expandedId === v.id;
            return (
              <div key={v.id} className="card" style={{ background: "var(--bg-soft)" }}>
                <div className="toolbar" style={{ padding: "12px 14px", flexWrap: "nowrap" }}>
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div className="toolbar" style={{ gap: 8 }}>
                      <b style={{ fontSize: 13.5 }}>{v.label}</b>
                      {v.recommended && <span className="badge badge-green">推荐</span>}
                      {v.files.length > 1 && (
                        <span className="badge">{v.files.length} 个分片</span>
                      )}
                    </div>
                    <div style={{ fontSize: 12, color: "var(--faint)", marginTop: 2 }}>
                      {formatBytes(v.totalSize)}
                      {v.companionsSize > 0 &&
                        ` + 配置/分词器等 ${formatBytes(v.companionsSize)}`}
                    </div>
                  </div>
                  <span className={RUN_LEVEL_CLASS[verdict.level]} title={verdict.desc}>
                    {verdict.label}
                  </span>
                  <button
                    className="btn-icon"
                    title="查看包含的文件"
                    onClick={() => setExpandedId(expanded ? null : v.id)}
                  >
                    {expanded ? "▲" : "▼"}
                  </button>
                  <button
                    className="btn btn-primary btn-sm"
                    disabled={starting}
                    onClick={() => download(v)}
                  >
                    <IconArrowDown size={13} /> 下载
                  </button>
                </div>

                {expanded && (
                  <div style={{ padding: "4px 14px 12px", fontSize: 12 }}>
                    {v.files.map((f) => (
                      <div key={f.path} className="toolbar" style={{ padding: "2px 0", justifyContent: "space-between" }}>
                        <span className="file-path">{f.path}</span>
                        <span style={{ color: "var(--faint)", fontVariantNumeric: "tabular-nums" }}>
                          {formatBytes(f.size)}
                        </span>
                      </div>
                    ))}
                    {v.companions.length > 0 && (
                      <>
                        <div style={{ margin: "8px 0 4px", color: "var(--faint)" }}>附带文件（自动包含）</div>
                        {v.companions.map((c) => (
                          <div key={c.path} className="toolbar" style={{ padding: "2px 0", justifyContent: "space-between" }}>
                            <span className="file-path" style={{ opacity: c.role === "mmproj" && !includeMmproj ? 0.45 : 1 }}>
                              {c.path}
                              {c.role === "mmproj" ? "（视觉投影）" : ""}
                            </span>
                            <span style={{ color: "var(--faint)", fontVariantNumeric: "tabular-nums" }}>
                              {formatBytes(c.size)}
                              {c.role === "mmproj" && !includeMmproj ? " · 已跳过" : ""}
                            </span>
                          </div>
                        ))}
                      </>
                    )}
                    <div style={{ marginTop: 6, color: "var(--faint)" }} title={verdict.desc}>
                      判定：{verdict.desc}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* advanced: raw file list */}
      <div className="card" style={{ marginTop: 14, overflow: "hidden" }}>
        <button
          className="toolbar"
          style={{
            width: "100%",
            padding: "13px 16px",
            justifyContent: "space-between",
            borderBottom: showAllFiles ? "1px solid var(--border-soft)" : "none",
          }}
          onClick={() => setShowAllFiles((v) => !v)}
        >
          <b style={{ fontSize: 13.5 }}>全部文件 · {data.files.length}</b>
          <span style={{ color: "var(--faint)", fontSize: 12 }}>{showAllFiles ? "收起 ▲" : "展开 ▼"}</span>
        </button>
        {showAllFiles && (
          <table className="file-table">
            <thead>
              <tr>
                <th style={{ width: "60%" }}>文件</th>
                <th>大小</th>
                <th style={{ textAlign: "right" }}>操作</th>
              </tr>
            </thead>
            <tbody>
              {data.files.map((f) => (
                <tr key={f.path}>
                  <td>
                    <span className="file-path">{f.path}</span>
                  </td>
                  <td style={{ fontVariantNumeric: "tabular-nums" }}>{formatBytes(f.size)}</td>
                  <td style={{ textAlign: "right" }}>
                    <button
                      className="btn btn-ghost btn-sm"
                      disabled={starting}
                      onClick={async () => {
                        try {
                          await api.startDownload(source, repo, f.path);
                          toast(`已开始下载：${f.path.split("/").pop()}`, "success");
                        } catch (e) {
                          toast(`下载失败：${String(e)}`, "error");
                        }
                      }}
                    >
                      下载
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* readme */}
      {data.readmeMd && (
        <div className="card readme" style={{ marginTop: 14 }}>
          <Markdown source={data.readmeMd} />
        </div>
      )}
    </div>
  );
}
