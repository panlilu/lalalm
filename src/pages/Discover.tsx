import { useCallback, useEffect, useRef, useState } from "react";
import { api, SOURCE_LABELS } from "../ipc";
import { searchCache } from "../cache";
import { repoWebUrl } from "../util";
import type { RecommendedItem } from "../types";
import {
  groupModels,
  classifyVariant,
  VARIANT_META,
  canonicalModelName,
  type ModelGroup,
} from "../util";
import { IconRefresh } from "../components/icons";
import { useStore } from "../store";
import type { ModelSummary, Source } from "../types";
import { formatCount, formatDate } from "../util";
import {
  avatarGradient,
  IconArrowDown,
  IconClock,
  IconHeart,
  IconSearch,
} from "../components/icons";

const SORTS: Array<{ key: string; label: string }> = [
  { key: "downloads", label: "最多下载" },
  { key: "trendingScore", label: "热门趋势" },
  { key: "likes", label: "最多点赞" },
  { key: "lastModified", label: "最近更新" },
];

// Module-level keep-alive cache so switching tabs away and back restores
// the selected source / filters / result list without re-fetching.
interface DiscoverCache {
  query: string;
  source: Source | undefined;
  sort: string;
  ggufOnly: boolean;
  results: ModelSummary[] | null;
  error: string | null;
}
let discoverCache: DiscoverCache = {
  query: "",
  source: undefined,
  sort: "downloads",
  ggufOnly: true,
  results: null,
  error: null,
};

function VariantGroupCard({
  group,
  links,
  onOpen,
}: {
  group: ModelGroup;
  links: Record<string, ChannelLinks>;
  onOpen: (repo: string, source: ModelSummary["source"]) => void;
}) {
  const rep =
    group.members.find((m) => classifyVariant(m.name) === "official") ??
    group.members[0];
  const publishers = new Set(group.members.map((m) => m.author)).size;
  // Aggregate channel availability across all members.
  const agg: ChannelLinks = {};
  for (const m of group.members) {
    const l: ChannelLinks | undefined = links[`${m.source}:${m.repo}`];
    if (!l) continue;
    agg.hf = agg.hf || l.hf === true || m.source !== "modelScope";
    agg.ms = agg.ms || l.ms === true || m.source === "modelScope";
  }
  return (
    <div className="model-card">
      <div className="mc-head">
        <OrgAvatar m={rep} />
        <div style={{ minWidth: 0 }}>
          <div className="mc-name">{canonicalModelName(rep.name)}</div>
          <div className="mc-author">
            {group.members.length} 个版本 · {publishers} 家发布者
          </div>
        </div>
      </div>
      <div
        className="toolbar"
        style={{ gap: 6, flexWrap: "wrap", margin: "8px 0", minHeight: 26 }}
      >
        {group.members.slice(0, 6).map((m) => {
          const kind = classifyVariant(m.name);
          const meta = VARIANT_META[kind];
          return (
            <button
              key={m.repo}
              className={`badge ${meta.cls}`}
              style={{ cursor: "pointer", border: "none" }}
              title={`${m.author}/${m.name} — 打开详情`}
              onClick={() => onOpen(m.repo, m.source)}
            >
              {meta.label}
              <span style={{ opacity: 0.75, marginLeft: 4 }}>{m.author}</span>
            </button>
          );
        })}
        {group.members.length > 6 && (
          <span className="badge">+{group.members.length - 6}</span>
        )}
      </div>
      <div className="toolbar" style={{ gap: 6, marginTop: 4 }} onClick={(e) => e.stopPropagation()}>
        <span style={{ fontSize: 11, color: "var(--faint)" }}>网页:</span>
        {CHANNELS.map(({ key, label, source }) => {
          const ok = agg[key] === true;
          return (
            <button
              key={key}
              className="chan-chip"
              data-ok={ok ? "1" : "0"}
              disabled={!ok}
              title={ok ? `在 ${label} 网页打开` : `${label} 未收录此模型`}
              onClick={() =>
                api.openUrl(repoWebUrl(source, rep.repo)).catch(() => {})
              }
            >
              {label}
            </button>
          );
        })}
        {agg.hf && (
          <button
            className="chan-chip"
            data-ok="1"
            title="在 hf-mirror 网页打开"
            onClick={() =>
              api.openUrl(repoWebUrl("hfMirror", rep.repo)).catch(() => {})
            }
          >
            镜像
          </button>
        )}
      </div>
    </div>
  );
}

function RecCard({ item, onOpen }: { item: RecommendedItem; onOpen: () => void }) {
  const name = item.repo.split("/").pop() ?? item.repo;
  const author = item.repo.split("/")[0] ?? "";
  const [avatar, setAvatar] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setAvatar(null);
    api
      .getOrgAvatar(item.source, author)
      .then((u) => alive && setAvatar(u))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [item.source, author]);
  return (
    <div className="model-card" onClick={onOpen}>
      <div className="mc-head">
        {avatar ? (
          <img
            className="mc-avatar"
            src={avatar}
            alt=""
            referrerPolicy="no-referrer"
            onError={() => setAvatar(null)}
          />
        ) : (
          <div className="mc-avatar" style={{ background: avatarGradient(item.repo) }}>
            {name.charAt(0)}
          </div>
        )}
        <div style={{ minWidth: 0 }}>
          <div className="mc-name">{name}</div>
          <div className="mc-author">{author}</div>
        </div>
      </div>
      <div
        style={{
          fontSize: 12,
          color: "var(--muted)",
          margin: "8px 0",
          minHeight: 34,
        }}
      >
        {item.note}
      </div>
      <div className="toolbar" style={{ gap: 6, flexWrap: "wrap" }}>
        <span className="badge badge-accent">{item.category}</span>
        {item.tags.map((t) => (
          <span key={t} className="badge">
            {t}
          </span>
        ))}
        <span className="badge" style={{ marginLeft: "auto" }}>
          {SOURCE_LABELS[item.source]}
        </span>
      </div>
    </div>
  );
}

function OrgAvatar({ m }: { m: ModelSummary }) {
  const [broken, setBroken] = useState(false);
  const letter = (m.name || "?").replace(/^models--?/i, "").charAt(0).toUpperCase();
  if (m.avatar && !broken) {
    return (
      <img
        className="mc-avatar"
        src={m.avatar}
        alt=""
        loading="lazy"
        referrerPolicy="no-referrer"
        onError={() => setBroken(true)}
      />
    );
  }
  return (
    <div className="mc-avatar" style={{ background: avatarGradient(m.repo) }}>
      {letter}
    </div>
  );
}

type ChannelLinks = { hf?: boolean; ms?: boolean };

const CHANNELS: Array<{ key: keyof ChannelLinks; label: string; source: Source }> = [
  { key: "ms", label: "MS", source: "modelScope" },
  { key: "hf", label: "HF", source: "huggingFace" },
];

function ModelCard({
  m,
  onOpen,
  links,
}: {
  m: ModelSummary;
  onOpen: () => void;
  links?: ChannelLinks;
}) {
  const { toast } = useStore();
  return (
    <div className="model-card" onClick={onOpen}>
      <div className="mc-head">
        <OrgAvatar m={m} />
        <div style={{ minWidth: 0 }}>
          <div className="mc-name">{m.name}</div>
          <div className="mc-author">{m.author || SOURCE_LABELS[m.source]}</div>
        </div>
      </div>
      <div className="mc-stats">
        <span title="下载量">
          <IconArrowDown />
          {formatCount(m.downloads)}
        </span>
        <span title="点赞">
          <IconHeart />
          {formatCount(m.likes)}
        </span>
        <span style={{ marginLeft: "auto" }}>{formatDate(m.lastModified)}</span>
      </div>
      <div className="mc-tags">
        {m.gguf && <span className="badge badge-cyan">GGUF</span>}
        {m.params && <span className="badge badge-accent">{m.params}</span>}
        {m.pipelineTag && <span className="badge">{m.pipelineTag}</span>}
        {m.tags
          .filter((t) => !["gguf", "text-generation-inference"].includes(t))
          .slice(0, 2)
          .map((t) => (
            <span key={t} className="badge">
              {t.length > 22 ? `${t.slice(0, 22)}…` : t}
            </span>
          ))}
      </div>
      <div className="toolbar" style={{ gap: 6, marginTop: 8 }} onClick={(e) => e.stopPropagation()}>
        <span style={{ fontSize: 11, color: "var(--faint)" }}>网页:</span>
        {CHANNELS.map(({ key, label, source }) => {
          const known = links?.[key];
          const ok = known === true;
          return (
            <button
              key={key}
              className="chan-chip"
              data-ok={ok ? "1" : "0"}
              disabled={known === undefined}
              title={
                known === undefined
                  ? "检查可用性中…"
                  : ok
                    ? `在 ${label} 网页打开`
                    : `${label} 未收录此模型`
              }
              onClick={() =>
                api.openUrl(repoWebUrl(source, m.repo)).catch((e) => toast(String(e), "error"))
              }
            >
              {label}
            </button>
          );
        })}
        {m.source !== "modelScope" && links?.hf === true && (
          <button
            className="chan-chip"
            data-ok="1"
            title="在 hf-mirror 网页打开"
            onClick={() =>
              api.openUrl(repoWebUrl("hfMirror", m.repo)).catch((e) => toast(String(e), "error"))
            }
          >
            镜像
          </button>
        )}
      </div>
    </div>
  );
}

export function Discover() {
  const { config, openDetail, toast } = useStore();
  const [query, setQuery] = useState(discoverCache.query);
  const [source, setSource] = useState<Source | undefined>(discoverCache.source);
  const [sort, setSort] = useState(discoverCache.sort);
  const [ggufOnly, setGgufOnly] = useState(discoverCache.ggufOnly);
  const [results, setResults] = useState<ModelSummary[] | null>(
    discoverCache.results
  );
  const [error, setError] = useState<string | null>(discoverCache.error);
  const [loading, setLoading] = useState(false);
  const [popOpen, setPopOpen] = useState(false);
  // Cross-channel availability for the cards on screen (filled lazily).
  const [linkMap, setLinkMap] = useState<Record<string, ChannelLinks>>({});
  const linkRunRef = useRef(0);
  const searchSeq = useRef(0);
  const lastKeyRef = useRef<string | null>(null);

  const effSource = source ?? config?.source ?? "huggingFace";

  // Write through to the keep-alive cache on every relevant change.
  useEffect(() => {
    discoverCache = {
      query,
      source,
      sort,
      ggufOnly,
      results,
      error,
    };
  }, [query, source, sort, ggufOnly, results, error]);

  const [stale, setStale] = useState(false);
  const [recs, setRecs] = useState<RecommendedItem[] | null>(null);
  const [homeTab, setHomeTab] = useState<"rec" | "hot">("rec");
  const { sysStats } = useStore();
  const [groupVariants, setGroupVariants] = useState(
    () => localStorage.getItem("lalalm.groupVariants") !== "0"
  );
  const toggleGroup = () => {
    setGroupVariants((v) => {
      localStorage.setItem("lalalm.groupVariants", v ? "0" : "1");
      return !v;
    });
  };

  useEffect(() => {
    api.getRecommended().then(setRecs).catch(() => setRecs([]));
  }, []);

  const doSearch = useCallback(
    async (q: string, opts?: { skipCache?: boolean }) => {
      const seq = ++searchSeq.current;
      // Remember the exact parameter set in flight so redundant effect
      // triggers (config arriving, remounts) don't double-fire.
      const cacheKey = `${effSource}|${sort}|${ggufOnly}|${q}`;
      lastKeyRef.current = cacheKey;
      setError(null);
      setQuery(q);
      setPopOpen(false);

      // Stale-while-revalidate: a cached list renders instantly and the
      // network refresh happens behind it. skipCache (manual refresh)
      // bypasses the instant render and always hits the network.
      const hit = opts?.skipCache ? undefined : searchCache.get(cacheKey);
      if (hit) {
        setResults(hit.results);
        setStale(true);
      } else {
        setResults(null);
        setStale(false);
      }
      setLoading(true);
      try {
        const list = await api.searchModels({
          source: effSource,
          query: q,
          sort,
          ggufOnly,
          limit: 36,
        });
        if (seq === searchSeq.current) {
          searchCache.put(cacheKey, list);
          setResults(list);
          setStale(false);
          if (list.length === 0) setError(null);
        }
      } catch (e) {
        if (seq === searchSeq.current) {
          // Keep cached results visible on network failure.
          if (!hit) {
            setResults([]);
            setError(String(e));
          } else {
            setStale(true);
            setLoading(false);
          }
          return;
        }
      } finally {
        if (seq === searchSeq.current) setLoading(false);
      }
    },
    [effSource, sort, ggufOnly]
  );

  // Background cross-channel availability check for visible results.
  useEffect(() => {
    if (!results || results.length === 0) return;
    const run = ++linkRunRef.current;
    setLinkMap({});
    let cancelled = false;
    (async () => {
      const pool = 6;
      const queue = [...results];
      const worker = async () => {
        while (queue.length > 0 && !cancelled) {
          const m = queue.shift()!;
          const key = `${m.source}:${m.repo}`;
          try {
            const [hf, ms] = await Promise.all([
              m.source === "modelScope"
                ? api.checkRepoExists("huggingFace", m.repo)
                : Promise.resolve(true),
              api.checkRepoExists("modelScope", m.repo),
            ]);
            if (cancelled) return;
            setLinkMap((prev) => ({ ...prev, [key]: { hf, ms } }));
          } catch {
            /* leave unknown */
          }
        }
      };
      await Promise.all(Array.from({ length: pool }, worker));
      void run;
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [results]);

  // Initial discovery list (only when there is nothing cached yet).
  useEffect(() => {
    if (config && results === null && !loading) {
      doSearch("");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config?.source]);

  // Re-run whenever the source / filters actually change. No `results`
  // guard here: switching tabs mid-load MUST cancel-and-restart the
  // search even when the first request hasn't returned yet (the seq
  // counter in doSearch discards the stale response).
  const firstFiltersRun = useRef(true);
  useEffect(() => {
    if (firstFiltersRun.current) {
      firstFiltersRun.current = false;
      return;
    }
    const key = `${effSource}|${sort}|${ggufOnly}|${query}`;
    if (key === lastKeyRef.current) return; // already in flight with these params
    doSearch(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sort, ggufOnly, effSource]);

  const recents = config?.recentSearches ?? [];
  const suggestions = config?.suggestQueries ?? [];
  const recMode = query === "" && homeTab === "rec";

  return (
    <div className="page-inner">
      <div style={{ marginBottom: 18 }}>
        <div className="page-title">发现模型</div>
        <div className="page-subtitle">
          搜索 Hugging Face / hf-mirror / ModelScope 上的 GGUF 大语言模型，一键下载到本机
        </div>
      </div>

      {/* search box */}
      <div className="search-wrap">
        <div className="search-box">
          <IconSearch size={19} />
          <input
            value={query}
            placeholder="搜索模型，例如：qwen2.5 · llama-3 · deepseek-r1 · phi-4 …"
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setPopOpen(true)}
            onBlur={() => window.setTimeout(() => setPopOpen(false), 180)}
            onKeyDown={(e) => {
              if (e.key === "Enter") doSearch(query);
              if (e.key === "Escape") setPopOpen(false);
            }}
          />
          <button
            className="btn btn-primary search-btn"
            onClick={() => doSearch(query)}
            disabled={loading}
          >
            搜索
          </button>
        </div>

        {popOpen && (
          <div className="search-pop">
            {recents.length > 0 && (
              <>
                <div className="pop-label">
                  最近搜索
                  <button
                    className="btn-icon"
                    onClick={() =>
                      config &&
                      api
                        .saveConfig({ ...config, recentSearches: [] })
                        .then(() => toast("已清除最近搜索"))
                    }
                    title="清除"
                  >
                    清除
                  </button>
                </div>
                {recents.map((r) => (
                  <button
                    key={r}
                    className="pop-item"
                    onMouseDown={() => doSearch(r)}
                  >
                    <IconClock />
                    {r}
                  </button>
                ))}
              </>
            )}
            <div className="pop-label">热门推荐</div>
            {suggestions.map((s) => (
              <button
                key={s}
                className="pop-item"
                onMouseDown={() => doSearch(s)}
              >
                <IconSearch size={14} />
                {s}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* home tabs (only before a query is typed) */}
      {query === "" && (
        <div className="toolbar" style={{ marginBottom: 12 }}>
          <div className="seg">
            <button className={homeTab === "rec" ? "on" : ""} onClick={() => setHomeTab("rec")}>
              ✦ 编辑推荐
            </button>
            <button className={homeTab === "hot" ? "on" : ""} onClick={() => setHomeTab("hot")}>
              热门发现
            </button>
          </div>
          <span style={{ color: "var(--faint)", fontSize: 12 }}>
            精选模型由配置文件维护 · 点击卡片查看量化与可运行性
          </span>
        </div>
      )}

      {/* recommended grid */}
      {query === "" && homeTab === "rec" && (
        recs === null ? (
          <div className="model-grid">
            {Array.from({ length: 8 }).map((_, i) => (
              <div key={i} className="skeleton" style={{ height: 150 }} />
            ))}
          </div>
        ) : (
          <div className="model-grid">
            {recs
              .filter(
                (it) =>
                  !it.platform ||
                  it.platform === sysStats?.platform ||
                  (it.platform === "macos" && sysStats?.arch === "aarch64")
              )
              .map((item) => (
                <RecCard
                  key={`${item.source}:${item.repo}`}
                  item={item}
                  onOpen={() => openDetail(item.repo, item.source)}
                />
              ))}
          </div>
        )
      )}

      {!recMode && (
      <>
      {/* toolbar */}
      <div className="toolbar" style={{ marginTop: query === "" ? 0 : 16 }}>
        <div className="seg">
          {(["modelScope", "huggingFace", "hfMirror"] as Source[]).map((s) => (
            <button
              key={s}
              className={effSource === s ? "on" : ""}
              onClick={() => setSource(s)}
              title={`从 ${SOURCE_LABELS[s]} 搜索`}
            >
              {SOURCE_LABELS[s]}
            </button>
          ))}
        </div>
        <select value={sort} onChange={(e) => setSort(e.target.value)}>
          {SORTS.map((s) => (
            <option key={s.key} value={s.key}>
              {s.label}
            </option>
          ))}
        </select>
        <button
          className={`chip${ggufOnly ? " on" : ""}`}
          onClick={() => setGgufOnly((v) => !v)}
        >
          仅 GGUF
        </button>
        {query !== "" && (
          <button
            className={`chip${groupVariants ? " on" : ""}`}
            onClick={toggleGroup}
            title="同一模型的多厂商量化 / 破限 / Raw 版本折叠为一张卡片"
          >
            聚合同模型
          </button>
        )}
        <div style={{ flex: 1 }} />
        <button
          className="btn-icon"
          title="强制刷新（忽略缓存）"
          disabled={loading}
          onClick={() => doSearch(query, { skipCache: true })}
        >
          <IconRefresh size={15} />
        </button>
        {results !== null && stale && (
          <span className="badge badge-warn" title="正在后台获取最新数据">
            缓存 · 刷新中…
          </span>
        )}
        {results !== null && !loading && (
          <span style={{ color: "var(--faint)", fontSize: 12.5 }}>
            共 {results.length} 个结果
          </span>
        )}
      </div>

      {/* results */}
      {error && (
        <div className="error-banner" style={{ marginTop: 16 }}>
          <span>搜索失败：{error}</span>
          <button className="btn btn-ghost btn-sm" onClick={() => doSearch(query)}>
            重试
          </button>
        </div>
      )}

      {loading && (
        <div className="model-grid">
          {Array.from({ length: 9 }).map((_, i) => (
            <div key={i} className="skeleton" style={{ height: 132 }} />
          ))}
        </div>
      )}

      {!error && results !== null && results.length === 0 && (
        <div className="empty-state">
          <div className="big">🔍</div>
          <h3>没有找到匹配的模型</h3>
          <p>换个关键词试试，或关闭「仅 GGUF」过滤。</p>
        </div>
      )}

      {results !== null && results.length > 0 && (
        <>
          {groupVariants ? (
            <div className="model-grid">
              {groupModels(results).map((g) => (
                <VariantGroupCard
                  key={g.key}
                  group={g}
                  links={linkMap}
                  onOpen={(repo, src) => openDetail(repo, src)}
                />
              ))}
            </div>
          ) : (
            <div className="model-grid">
              {results.map((m) => (
                <ModelCard
                  key={`${m.source}:${m.repo}`}
                  m={m}
                  links={linkMap[`${m.source}:${m.repo}`]}
                  onOpen={() => openDetail(m.repo, m.source)}
                />
              ))}
            </div>
          )}
        </>
      )}

      {results === null && !loading && (
        <div className="empty-state">
          <div className="spinner" />
        </div>
      )}
      </>
      )}
    </div>
  );
}
