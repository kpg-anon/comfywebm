// Typed wrappers for every command and event in the PLAN.md contract (v2).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConfigInfo,
  Crop,
  DownloadProgress,
  EncodeSpec,
  Job,
  MediaInfo,
  PlanInfo,
  Preset,
  ProxyProgress,
  QueueState,
  Segment,
  Settings,
  ToolsStatus,
  UrlInfo,
} from "./types";

export const toolsStatus = () => invoke<ToolsStatus>("tools_status");
/** Searches for the tools again and returns the new status. */
export const toolsRescan = () => invoke<ToolsStatus>("tools_rescan");
export const initialFile = () => invoke<string | null>("initial_file");
export const openItem = (path: string) => invoke<void>("open_item", { path });
export const probe = (path: string) => invoke<MediaInfo>("probe", { path });
/** `remux` false: skip the stream copy (it was tried and did not play). */
export const makeProxy = (path: string, remux = true) => invoke<{ path: string; remuxed: boolean }>("make_proxy", { path, remux });
export const plan = (spec: EncodeSpec) => invoke<PlanInfo>("plan", { spec });
export const queueList = () => invoke<Job[]>("queue_list");
export const queueAdd = (spec: EncodeSpec, start: boolean) => invoke<Job>("queue_add", { spec, start });
export const queueAddMany = (specs: EncodeSpec[], start: boolean) => invoke<Job[]>("queue_add_many", { specs, start });
export const queueStart = () => invoke<void>("queue_start");
export const queueCancel = (id: number) => invoke<void>("queue_cancel", { id });
export const queueRemove = (id: number) => invoke<void>("queue_remove", { id });
/** Stop after the running job. */
export const queuePause = () => invoke<void>("queue_pause");
export const queueState = () => invoke<QueueState>("queue_state");
/** A finished job goes back to pending at the end of the list. */
export const queueRetry = (id: number) => invoke<Job>("queue_retry", { id });
/** Moves a job to list position `to`; returns the new list. */
export const queueMove = (id: number, to: number) => invoke<Job[]>("queue_move", { id, to });
export const queueRemoveWaiting = () => invoke<Job[]>("queue_remove_waiting");
export const sampleCancel = () => invoke<void>("sample_cancel");
/** Black bars as a crop in display pixels; null = none found. */
export const detectCrop = (path: string) => invoke<Crop | null>("detect_crop", { path });
/** The frame at `time` with every filter and the output size applied, as a PNG. */
export const exportFrameProcessed = (spec: EncodeSpec, time: number) => invoke<string>("export_frame_processed", { spec, time });
/** Filmstrip JPEG / waveform PNG for the timeline; paths in the cache dir. */
export const timelineThumbs = (path: string) => invoke<string>("timeline_thumbs", { path });
export const proxyCancel = () => invoke<void>("proxy_cancel");
export const poster = (path: string) => invoke<string>("poster", { path });
/** The embedded cover picture of a sound file, as a PNG in the cache dir. */
export const coverPicture = (path: string) => invoke<string>("cover_picture", { path });
export const sceneCuts = (path: string) => invoke<number[]>("scene_cuts", { path });
export const timelineWave = (path: string, track: number) => invoke<string>("timeline_wave", { path, track });
export type CacheInfo = { dir: string; bytes: number; files: number; downloadBytes: number };
export const cacheInfo = () => invoke<CacheInfo>("cache_info");
export const cacheClear = (downloads: boolean) => invoke<void>("cache_clear", { downloads });
export const settingsGet = () => invoke<Settings>("settings_get");
export const settingsSet = (settings: Settings) => invoke<void>("settings_set", { settings });

export const urlInfo = (url: string) => invoke<UrlInfo>("url_info", { url });
/** `keep`: "Download only": the original goes to the keep folder in its own container. */
export const urlDownload = (url: string, maxHeight: number | null, section: Segment | null, keep = false) =>
  invoke<string>("url_download", { url, maxHeight, section, keep });
export const urlCancel = (url: string) => invoke<void>("url_cancel", { url });
export const exportFrame = (path: string, time: number, crop: Crop | null) =>
  invoke<string>("export_frame", { path, time, crop });
export const previewSample = (spec: EncodeSpec, at: number, secs = 3) => invoke<string>("preview_sample", { spec, at, secs });
export const presetsExport = (path: string, presets: Preset[]) => invoke<void>("presets_export", { path, presets });
export const presetsImport = (path: string) => invoke<Preset[]>("presets_import", { path });

export const configGet = () => invoke<ConfigInfo>("config_get");
export const presetSave = (preset: Preset, file: string | null) => invoke<string>("preset_save", { preset, file });
export const presetDelete = (file: string) => invoke<void>("preset_delete", { file });
export const configRestorePlatform = () => invoke<void>("config_restore_platform");
export const outputDir = () => invoke<string | null>("output_dir");
export const listMedia = (dir: string, recursive: boolean) => invoke<string[]>("list_media", { dir, recursive });
/** The file dialogs run in Rust; null = canceled. */
export const themeExport = (text: string) => invoke<string | null>("theme_export", { text });
export const themeImport = () => invoke<string | null>("theme_import");

export const onJobUpdate = (cb: (job: Job) => void): Promise<UnlistenFn> =>
  listen<Job>("job-update", (e) => cb(e.payload));
export const onQueueState = (cb: (s: QueueState) => void): Promise<UnlistenFn> =>
  listen<QueueState>("queue-state", (e) => cb(e.payload));
export const onProxyProgress = (cb: (p: ProxyProgress) => void): Promise<UnlistenFn> =>
  listen<ProxyProgress>("proxy-progress", (e) => cb(e.payload));
export const onDownloadProgress = (cb: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
  listen<DownloadProgress>("download-progress", (e) => cb(e.payload));

/** Commands return errors as a plain string; anything else is turned into one. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
