// Typed wrappers around Tauri invoke commands.
import { invoke } from "@tauri-apps/api/core";
import type {
  CachePathInfo,
  Config,
  DownloadTask,
  LocalModel,
  ModelDetail,
  ModelSummary,
  OpResult,
  RecommendedItem,
  Source,
  SysStats,
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<Config>("save_config", { config }),
  getCachePaths: () => invoke<CachePathInfo[]>("get_cache_paths"),

  searchModels: (args: {
    source: Source;
    query: string;
    sort?: string;
    ggufOnly?: boolean;
    limit?: number;
  }) => invoke<ModelSummary[]>("search_models", args),

  getModelDetail: (source: Source, repo: string) =>
    invoke<ModelDetail>("get_model_detail", { source, repo }),

  startDownload: (source: Source, repo: string, path: string) =>
    invoke<DownloadTask>("start_download", { source, repo, path }),
  startDownloadBatch: (source: Source, repo: string, paths: string[]) =>
    invoke<number>("start_download_batch", { source, repo, paths }),
  listDownloads: () => invoke<DownloadTask[]>("list_downloads"),
  pauseDownload: (id: string) => invoke<void>("pause_download", { id }),
  resumeDownload: (id: string) => invoke<void>("resume_download", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  retryDownload: (id: string) => invoke<DownloadTask>("retry_download", { id }),
  removeDownload: (id: string) => invoke<void>("remove_download", { id }),
  clearFinishedDownloads: () => invoke<number>("clear_finished_downloads"),

  listLocalModels: () => invoke<LocalModel[]>("list_local_models"),
  deleteLocalModels: (paths: string[]) =>
    invoke<OpResult[]>("delete_local_models", { paths }),
  moveLocalModels: (paths: string[], destDir: string) =>
    invoke<OpResult[]>("move_local_models", { paths, destDir }),
  dirSizes: (paths: string[]) =>
    invoke<Record<string, number>>("dir_sizes", { paths }),

  pickFolder: () => invoke<string | null>("pick_folder"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  checkRepoExists: (source: Source, repo: string) =>
    invoke<boolean>("check_repo_exists", { source, repo }),
  getLmStudioDir: () => invoke<string>("lm_studio_dir"),
  getRecommended: () => invoke<RecommendedItem[]>("get_recommended"),
  readAria2Log: (lines?: number) =>
    invoke<string>("read_aria2_log", { lines: lines ?? 200 }),
  getSysStats: () => invoke<SysStats>("get_sys_stats"),
  getAppVersion: () => invoke<string>("get_app_version"),
};

export const SOURCE_LABELS: Record<Source, string> = {
  huggingFace: "Hugging Face",
  hfMirror: "hf-mirror",
  modelScope: "ModelScope",
};
