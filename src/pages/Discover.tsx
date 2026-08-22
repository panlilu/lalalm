import { useCallback, useEffect, useRef, useState } from "react";
import { api, SOURCE_LABELS } from "../ipc";
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

function ModelCard({ m, onOpen }: { m: ModelSummary; onOpen: () => void }) {
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

  const doSearch = useCallback(
    async (q: string) => {
      const seq = ++searchSeq.current;
      // Remember the exact parameter set in flight so redundant effect
      // triggers (config arriving, remounts) don't double-fire.
      lastKeyRef.current = `${effSource}|${sort}|${ggufOnly}|${q}`;
      setLoading(true);
      setError(null);
      setQuery(q);
      setPopOpen(false);
      try {
        const list = await api.searchModels({
          source: effSource,
          query: q,
          sort,
          ggufOnly,
          limit: 36,
        });
        if (seq === searchSeq.current) {
          setResults(list);
          if (list.length === 0) setError(null);
        }
      } catch (e) {
        if (seq === searchSeq.current) {
          setResults([]);
          setError(String(e));
        }
      } finally {
        if (seq === searchSeq.current) setLoading(false);
      }
    },
    [effSource, sort, ggufOnly]
  );

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

      {/* toolbar */}
      <div className="toolbar" style={{ marginTop: 16 }}>
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
        <div style={{ flex: 1 }} />
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

      {!loading && results !== null && results.length === 0 && !error && (
        <div className="empty-state">
          <div className="big">🔍</div>
          <h3>没有找到匹配的模型</h3>
          <p>换个关键词试试，或关闭「仅 GGUF」过滤。</p>
        </div>
      )}

      {!loading && results !== null && results.length > 0 && (
        <div className="model-grid">
          {results.map((m) => (
            <ModelCard
              key={`${m.source}:${m.repo}`}
              m={m}
              onOpen={() => openDetail(m.repo, m.source)}
            />
          ))}
        </div>
      )}

      {results === null && !loading && (
        <div className="empty-state">
          <div className="spinner" />
        </div>
      )}
    </div>
  );
}
