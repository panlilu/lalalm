// Small formatting helpers shared across pages.

export function formatBytes(n?: number | null, digits = 1): string {
  if (n === undefined || n === null || isNaN(n)) return "—";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(digits)} ${units[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec <= 0) return "0 KB/s";
  return `${formatBytes(bytesPerSec)}/s`;
}

export function formatCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n ?? 0);
}

export function formatDate(s?: string | number | null): string {
  if (!s) return "—";
  const d = typeof s === "number" ? new Date(s * 1000) : new Date(s);
  if (isNaN(d.getTime())) return String(s);
  return d.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function formatEta(seconds: number): string {
  if (!isFinite(seconds) || seconds <= 0) return "—";
  if (seconds < 60) return `${Math.ceil(seconds)} 秒`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m} 分 ${Math.ceil(seconds % 60)} 秒`;
  const h = Math.floor(m / 60);
  return `${h} 时 ${m % 60} 分`;
}

export function percent(downloaded: number, total: number): number {
  if (!total || total <= 0) return 0;
  return Math.min(100, (downloaded / total) * 100);
}

const QUANT_ORDER = [
  "F32", "BF16", "F16", "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q5_1",
  "Q4_K_M", "Q4_K_S", "IQ4_XS", "IQ4_NL", "Q4_0", "Q4_1", "Q3_K_L", "Q3_K_M",
  "Q3_K_S", "IQ3_M", "IQ3_S", "IQ3_XS", "IQ3_XXS", "Q2_K", "Q2_K_S", "IQ2",
  "IQ1",
];

/** Rank a quantization label by practical quality preference. */
export function quantRank(q?: string | null): number {
  if (!q) return 999;
  const up = q.toUpperCase();
  const idx = QUANT_ORDER.findIndex((k) => up === k || up.startsWith(k));
  return idx >= 0 ? idx : 500;
}

export function isRecommendedQuant(q?: string | null): boolean {
  if (!q) return false;
  const up = q.toUpperCase();
  return up.startsWith("Q4_K") || up === "Q4_0" || up === "Q4_1" ||
         up.startsWith("IQ4");
}

export const ORIGIN_LABELS: Record<string, string> = {
  "lalalm-library": "LalaLM 库",
  "huggingface-cache": "HF 缓存",
  "lm-studio": "LM Studio",
  "modelscope-cache": "ModelScope 缓存",
  custom: "自定义路径",
};

export type RunLevel = "fine" | "ok" | "tight" | "no" | "unknown";

export interface RunVerdict {
  level: RunLevel;
  label: string;
  desc: string;
}

/**
 * Estimate whether a model of `sizeBytes` can run on this machine.
 * Weights need ~1.25× their file size once compute buffers and KV cache are
 * accounted for. Beyond that, llama.cpp-style mmap lets slightly oversized
 * models run partially from disk (slower), while truly oversized ones OOM.
 */
export function assessRun(sizeBytes: number, memTotal?: number | null): RunVerdict {
  if (!memTotal || memTotal <= 0) {
    return { level: "unknown", label: "评估中…", desc: "正在获取本机内存信息" };
  }
  const need = sizeBytes * 1.25;
  if (need <= memTotal * 0.6) {
    return { level: "fine", label: "流畅运行", desc: "内存充裕，可完整加载并留有充足余量" };
  }
  if (need <= memTotal * 0.85) {
    return { level: "ok", label: "可以运行", desc: "完整加载后仍可运行，建议关闭其他大内存应用" };
  }
  if (sizeBytes <= memTotal * 0.95) {
    return {
      level: "tight",
      label: "部分加载可跑",
      desc: "超出可用内存，将依赖 mmap 部分加载（速度受磁盘影响）",
    };
  }
  return { level: "no", label: "大概率 OOM", desc: "权重超过本机内存，无法完整加载，请选择更小的量化" };
}

export const RUN_LEVEL_CLASS: Record<RunLevel, string> = {
  fine: "badge badge-green",
  ok: "badge badge-accent",
  tight: "badge badge-warn",
  no: "badge badge-red",
  unknown: "badge",
};

export function formatLabel(format: string): string {
  switch (format) {
    case "gguf":
      return "GGUF · llama.cpp";
    case "mlx":
      return "MLX · Apple";
    default:
      return "Safetensors · Transformers";
  }
}

const SOURCE_HOME: Record<string, string> = {
  huggingFace: "https://huggingface.co",
  hfMirror: "https://hf-mirror.com",
  modelScope: "https://modelscope.cn/models",
};

/** Web page of a model on its hub. */
export function repoWebUrl(source: string, repo: string): string {
  return `${SOURCE_HOME[source] ?? SOURCE_HOME.huggingFace}/${repo}`;
}
