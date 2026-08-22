// Shared types mirroring the Rust backend's serde models.

export type Source = "huggingFace" | "hfMirror" | "modelScope";

export interface ModelSummary {
  repo: string;
  author: string;
  name: string;
  source: Source;
  downloads: number;
  likes: number;
  lastModified: string;
  tags: string[];
  pipelineTag?: string | null;
  gguf: boolean;
  params?: string | null;
  avatar?: string | null;
}

export type FileRole = "weights" | "mmproj" | "config" | "tokenizer" | "other";

export interface ModelFile {
  path: string;
  size: number;
  quant?: string | null;
  isGguf: boolean;
  role: FileRole;
}

export type ModelFormat = "gguf" | "mlx" | "tensor";

/** A user-facing downloadable unit: one quant / weight set + companions. */
export interface Variant {
  id: string;
  label: string;
  quant?: string | null;
  files: ModelFile[];
  totalSize: number;
  companions: ModelFile[];
  companionsSize: number;
  recommended: boolean;
}

export interface ModelDetail {
  summary: ModelSummary;
  format: ModelFormat;
  variants: Variant[];
  files: ModelFile[];
  readmeMd?: string | null;
  ggufTotalSize: number;
  allTotalSize: number;
}

export type DlStatus =
  | "queued"
  | "active"
  | "paused"
  | "completed"
  | "error"
  | "cancelled"
  | "interrupted";

export interface DownloadTask {
  id: string;
  repo: string;
  path: string;
  source: Source;
  url: string;
  dir: string;
  out: string;
  total: number;
  downloaded: number;
  speed: number;
  status: DlStatus;
  error?: string | null;
  gid?: string | null;
  addedAt: number;
  updatedAt: number;
}

export interface GgufMeta {
  architecture?: string | null;
  name?: string | null;
  sizeLabel?: string | null;
  quant?: string | null;
  contextLength?: number | null;
  blockCount?: number | null;
  embeddingLength?: number | null;
  headCount?: number | null;
  fileVersion?: number | null;
}

export type LocalOrigin =
  | "lalalm-library"
  | "huggingface-cache"
  | "lm-studio"
  | "modelscope-cache"
  | "custom";

export interface LocalModel {
  id: string;
  name: string;
  repo?: string | null;
  family: string;
  quant?: string | null;
  filePath: string;
  fileName: string;
  size: number;
  modified: number;
  origin: LocalOrigin;
  kind: string;
  meta?: GgufMeta | null;
}

export interface SysStats {
  cpuUsage: number;
  cpuCount: number;
  memTotal: number;
  memUsed: number;
  memPercent: number;
  swapUsed: number;
  vramTotal?: number | null;
  vramUnified: boolean;
  gpuName?: string | null;
  diskFree: number;
  diskTotal: number;
  platform: string;
  arch: string;
}

export interface CachePathInfo {
  label: string;
  path: string;
  exists: boolean;
  scanned: boolean;
}

export interface Aria2Config {
  enabled: boolean;
  maxConnectionPerServer: number;
  split: number;
  minSplitSize: string;
  maxConcurrentDownloads: number;
}

export type ProxyMode = "system" | "direct" | "manual";
export type DownloadDestination = "library" | "lmStudio";

export interface Config {
  source: Source;
  hfToken: string;
  modelscopeToken: string;
  downloadDir: string;
  searchPaths: string[];
  scanHfCache: boolean;
  scanLmStudio: boolean;
  scanModelscopeCache: boolean;
  recentSearches: string[];
  suggestQueries: string[];
  aria2: Aria2Config;
  proxyMode: ProxyMode;
  proxyUrl: string;
  downloadDestination: DownloadDestination;
}

export interface OpResult {
  path: string;
  to?: string | null;
  ok: boolean;
  error?: string | null;
}
