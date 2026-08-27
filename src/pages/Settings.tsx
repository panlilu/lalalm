import { useEffect, useState } from "react";
import { api, SOURCE_LABELS } from "../ipc";
import { useStore } from "../store";
import type { Config, Source } from "../types";
import { formatBytes } from "../util";

function AboutCard() {
  const { toast, updateInfo, setUpdateInfo } = useStore();
  const [checking, setChecking] = useState(false);
  const [ver, setVer] = useState("");
  useEffect(() => {
    api.getAppVersion().then(setVer).catch(() => {});
  }, []);
  return (
    <div className="toolbar" style={{ flexWrap: "wrap", gap: 10 }}>
      <span>
        当前版本 <b>v{ver || "…"}</b>
        {updateInfo && (
          <span style={{ color: "var(--accent-2)", marginLeft: 8 }}>
            · 可更新到 v{updateInfo.latest}（顶部横幅可一键更新）
          </span>
        )}
      </span>
      <div style={{ flex: 1 }} />
      <button
        className="btn btn-ghost btn-sm"
        disabled={checking}
        onClick={async () => {
          setChecking(true);
          try {
            const u = await api.checkUpdate();
            if (u) {
              setUpdateInfo(u);
              toast(`发现新版本 v${u.latest}`, "success");
            } else {
              toast("已是最新版本 ✓");
            }
          } catch (e) {
            toast(`检查失败：${String(e)}`, "error");
          } finally {
            setChecking(false);
          }
        }}
      >
        {checking ? "检查中…" : "检查更新"}
      </button>
    </div>
  );
}

export function Settings() {
  const { config, toast, sysStats, reloadConfig } = useStore();
  const [draft, setDraft] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [lmsDir, setLmsDir] = useState<string | null>(null);

  useEffect(() => {
    if (config && draft === null) setDraft(structuredClone(config));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  useEffect(() => {
    api.getLmStudioDir().then(setLmsDir).catch(() => {});
  }, []);

  if (!draft) {
    return (
      <div className="page-inner">
        <div className="page-title">设置</div>
        <div className="skeleton" style={{ height: 300, marginTop: 16 }} />
      </div>
    );
  }

  const patch = (p: Partial<Config>) => setDraft({ ...draft, ...p });
  const dirty = JSON.stringify(draft) !== JSON.stringify(config);

  /** Persist a full config snapshot right now (used for instant-apply fields). */
  const persist = async (snapshot: Config) => {
    try {
      const saved = await api.saveConfig(snapshot);
      setDraft(structuredClone(saved));
      await reloadConfig();
      toast("已保存", "success");
    } catch (e) {
      toast(`保存失败：${String(e)}`, "error");
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const saved = await api.saveConfig(draft);
      setDraft(structuredClone(saved));
      await reloadConfig();
      toast("设置已保存", "success");
    } catch (e) {
      toast(`保存失败：${String(e)}`, "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page-inner">
      <div style={{ marginBottom: 18, display: "flex", alignItems: "flex-end" }}>
        <div style={{ flex: 1 }}>
          <div className="page-title">设置</div>
          <div className="page-subtitle">下载来源、aria2 并发、存储位置与本地扫描</div>
        </div>
        {dirty && (
          <button className="btn btn-primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存设置"}
          </button>
        )}
      </div>

      {/* about */}
      <div className="card settings-section">
        <h3>关于</h3>
        <AboutCard />
      </div>

      {/* appearance */}
      <div className="card settings-section">
        <h3>外观</h3>
        <div className="desc">深色 / 浅色主题。「跟随系统」会在系统切换外观时实时联动。</div>
        <div style={{ display: "flex", gap: 10 }}>
          {(
            [
              ["system", "跟随系统", "与操作系统外观保持一致"],
              ["dark", "深色", "默认深色主题"],
              ["light", "浅色", "明亮主题"],
            ] as const
          ).map(([m, label, hint]) => (
            <button
              key={m}
              className={`source-radio${(draft.theme ?? "system") === m ? " on" : ""}`}
              onClick={async () => {
                if (draft.theme === m) return;
                const next = { ...draft, theme: m };
                setDraft(next);
                await persist(next);
              }}
            >
              <b>{label}</b>
              <span>{hint}</span>
            </button>
          ))}
        </div>
      </div>

      {/* download source */}
      <div className="card settings-section">
        <h3>模型下载来源</h3>
        <div className="desc">选择模型搜索与下载使用的远端仓库。国内网络推荐 hf-mirror 或 ModelScope。</div>
        <div style={{ display: "flex", gap: 10 }}>
          {(Object.keys(SOURCE_LABELS) as Source[]).map((s) => (
            <button
              key={s}
              className={`source-radio${draft.source === s ? " on" : ""}`}
              onClick={() => patch({ source: s })}
            >
              <b>{SOURCE_LABELS[s]}</b>
              <span>
                {s === "huggingFace" && "官方源 · 模型最全"}
                {s === "hfMirror" && "HuggingFace 国内镜像 · 免代理"}
                {s === "modelScope" && "阿里魔搭 · 国内速度快"}
              </span>
            </button>
          ))}
        </div>
        <div className="form-row" style={{ marginTop: 12 }}>
          <label>Hugging Face Token</label>
          <input
            type="password"
            placeholder="hf_…（下载 gated 模型时需要）"
            value={draft.hfToken}
            onChange={(e) => patch({ hfToken: e.target.value })}
            style={{ flex: 1 }}
          />
        </div>
        <div className="form-row">
          <label>ModelScope Token</label>
          <input
            type="password"
            placeholder="一般无需填写"
            value={draft.modelscopeToken}
            onChange={(e) => patch({ modelscopeToken: e.target.value })}
            style={{ flex: 1 }}
          />
        </div>
      </div>

      {/* network proxy */}
      <div className="card settings-section">
        <h3>网络代理</h3>
        <div className="desc">
          模型搜索与 aria2c 下载使用的代理。默认跟随系统代理（macOS 网络设置 /
          Windows Internet 设置 / 环境变量）。
        </div>
        <div style={{ display: "flex", gap: 10 }}>
          {(
            [
              ["system", "跟随系统", "读取系统网络代理设置"],
              ["direct", "直连", "不使用任何代理"],
              ["manual", "手动配置", "自定义代理地址"],
            ] as const
          ).map(([m, label, hint]) => (
            <button
              key={m}
              className={`source-radio${draft.proxyMode === m ? " on" : ""}`}
              onClick={() => patch({ proxyMode: m })}
            >
              <b>{label}</b>
              <span>{hint}</span>
            </button>
          ))}
        </div>
        {draft.proxyMode === "manual" && (
          <div className="form-row" style={{ marginTop: 12 }}>
            <label>代理地址</label>
            <input
              placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
              value={draft.proxyUrl}
              onChange={(e) => patch({ proxyUrl: e.target.value })}
              style={{ flex: 1 }}
            />
          </div>
        )}
      </div>

      {/* storage */}
      <div className="card settings-section">
        <h3>下载位置</h3>
        <div className="desc">
          可存入 LalaLM 自己的模型库，或直接下载到 LM Studio 模型目录（按
          发布者/模型/文件 组织，LM Studio 打开即可使用）。
        </div>
        <div style={{ display: "flex", gap: 10 }}>
          <button
            className={`source-radio${draft.downloadDestination === "library" ? " on" : ""}`}
            onClick={() => patch({ downloadDestination: "library" })}
          >
            <b>LalaLM 模型库</b>
            <span>{draft.downloadDir}</span>
          </button>
          <button
            className={`source-radio${draft.downloadDestination === "lmStudio" ? " on" : ""}`}
            onClick={() => patch({ downloadDestination: "lmStudio" })}
          >
            <b>LM Studio 目录</b>
            <span>
              {lmsDir || "~/.lmstudio/models"} · 已读取 LM Studio 配置 ·
              自动开启该目录扫描
            </span>
          </button>
        </div>
        {draft.downloadDestination === "library" && (
          <div className="form-row" style={{ marginTop: 12 }}>
            <label>库目录</label>
            <code className="path-item" style={{ flex: 1 }}>
              {draft.downloadDir}
            </code>
            <button
              className="btn btn-ghost btn-sm"
              onClick={async () => {
                const dir = await api.pickFolder();
                if (dir) patch({ downloadDir: dir });
              }}
            >
              选择目录…
            </button>
          </div>
        )}
        {sysStats && (
          <div className="hint" style={{ fontSize: 11.5, color: "var(--faint)" }}>
            所在磁盘剩余空间：{formatBytes(sysStats.diskFree)} / 共 {formatBytes(sysStats.diskTotal)}
          </div>
        )}
      </div>

      {/* aria2 */}
      <div className="card settings-section">
        <h3>aria2c 下载引擎</h3>
        <div className="desc">应用内置 aria2c，多线程分块加速大文件下载；修改并发参数后保存即生效（下一次下载生效）。</div>
        <div className="form-row">
          <label>启用 aria2c 加速</label>
          <input
            type="checkbox"
            className="switch"
            checked={draft.aria2.enabled}
            onChange={(e) =>
              patch({ aria2: { ...draft.aria2, enabled: e.target.checked } })
            }
          />
        </div>
        <div className="form-row">
          <label>单服务器连接数</label>
          <input
            type="range"
            min={1}
            max={32}
            value={draft.aria2.maxConnectionPerServer}
            onChange={(e) =>
              patch({
                aria2: { ...draft.aria2, maxConnectionPerServer: Number(e.target.value) },
              })
            }
          />
          <span className="range-val">{draft.aria2.maxConnectionPerServer}</span>
        </div>
        <div className="form-row">
          <label>单任务分块数</label>
          <input
            type="range"
            min={1}
            max={32}
            value={draft.aria2.split}
            onChange={(e) => patch({ aria2: { ...draft.aria2, split: Number(e.target.value) } })}
          />
          <span className="range-val">{draft.aria2.split}</span>
        </div>
        <div className="form-row">
          <label>最小分块大小</label>
          <select
            value={draft.aria2.minSplitSize}
            onChange={(e) =>
              patch({ aria2: { ...draft.aria2, minSplitSize: e.target.value } })
            }
            style={{ width: 120 }}
          >
            {["1M", "4M", "8M", "16M", "32M"].map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>
        </div>
        <div className="form-row">
          <label>同时下载数上限</label>
          <input
            type="range"
            min={1}
            max={10}
            value={draft.aria2.maxConcurrentDownloads}
            onChange={(e) =>
              patch({
                aria2: { ...draft.aria2, maxConcurrentDownloads: Number(e.target.value) },
              })
            }
          />
          <span className="range-val">{draft.aria2.maxConcurrentDownloads}</span>
        </div>
      </div>

      {/* scanning */}
      <div className="card settings-section">
        <h3>本机扫描路径</h3>
        <div className="desc">「本地模型」页面会扫描以下缓存与自定义目录，自动识别 GGUF 权重。</div>
        <div className="form-row">
          <label>Hugging Face 缓存</label>
          <span className="hint" style={{ flex: 1 }}>
            ~/.cache/huggingface/hub
          </span>
          <input
            type="checkbox"
            className="switch"
            checked={draft.scanHfCache}
            onChange={(e) => patch({ scanHfCache: e.target.checked })}
          />
        </div>
        <div className="form-row">
          <label>LM Studio 目录</label>
          <span className="hint" style={{ flex: 1 }}>
            ~/.lmstudio/models
          </span>
          <input
            type="checkbox"
            className="switch"
            checked={draft.scanLmStudio}
            onChange={(e) => patch({ scanLmStudio: e.target.checked })}
          />
        </div>
        <div className="form-row">
          <label>ModelScope 缓存</label>
          <span className="hint" style={{ flex: 1 }}>
            ~/.cache/modelscope/hub
          </span>
          <input
            type="checkbox"
            className="switch"
            checked={draft.scanModelscopeCache}
            onChange={(e) => patch({ scanModelscopeCache: e.target.checked })}
          />
        </div>
        <div className="divider" />
        <b style={{ fontSize: 13 }}>自定义搜索路径</b>
        <div className="path-list" style={{ marginTop: 10 }}>
          {draft.searchPaths.length === 0 && (
            <span className="hint" style={{ color: "var(--faint)", fontSize: 12.5 }}>
              尚未添加自定义路径
            </span>
          )}
          {draft.searchPaths.map((p) => (
            <div key={p} className="path-item">
              <code>{p}</code>
              <button
                className="btn-icon"
                title="移除"
                onClick={async () => {
                  const next = draft.searchPaths.filter((x) => x !== p);
                  setDraft({ ...draft, searchPaths: next });
                  await persist({ ...draft, searchPaths: next });
                }}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
        <button
          className="btn btn-ghost btn-sm"
          style={{ marginTop: 10 }}
          onClick={async () => {
            const dir = await api.pickFolder();
            if (dir && !draft.searchPaths.includes(dir)) {
              const next = [...draft.searchPaths, dir];
              setDraft({ ...draft, searchPaths: next });
              await persist({ ...draft, searchPaths: next });
            }
          }}
        >
          ＋ 添加路径…
        </button>
      </div>

      {/* about */}
      <div className="card settings-section">
        <h3>关于</h3>
        <div className="desc">
          LalaLM v0.1.0 · Tauri 构建 · 内置 aria2c 1.37.0 · 当前平台 macOS ({sysStats?.arch})
        </div>
        <div className="hint" style={{ fontSize: 12, color: "var(--muted)" }}>
          与 LM Studio、llama.cpp、Hugging Face CLI 等工具的默认模型目录互相兼容，可随时切换使用。
        </div>
      </div>
    </div>
  );
}
