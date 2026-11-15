// Native file and folder pickers (@tauri-apps/plugin-dialog).
import { open, save } from "@tauri-apps/plugin-dialog";

const VIDEO_EXT = ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ts", "m2ts", "mts", "flv", "wmv", "mpg", "mpeg", "ogv", "3gp", "gif"];

async function pick(opts: Parameters<typeof open>[0]): Promise<string | null> {
  const r = await open({ multiple: false, ...opts });
  return typeof r === "string" ? r : null;
}

export const pickVideo = () =>
  pick({ title: "Open video", filters: [{ name: "Video", extensions: VIDEO_EXT }, { name: "All files", extensions: ["*"] }] });

export const pickSubtitle = () =>
  pick({ title: "Choose subtitle file", filters: [{ name: "Subtitles", extensions: ["srt", "ass", "ssa", "vtt"] }, { name: "All files", extensions: ["*"] }] });

export const pickFolder = (title: string) => pick({ title, directory: true });

export const pickAnyFile = (title: string) => pick({ title });

export const pickVideos = async (): Promise<string[]> => {
  const r = await open({
    multiple: true,
    title: "Open video",
    filters: [{ name: "Video", extensions: VIDEO_EXT }, { name: "All files", extensions: ["*"] }],
  });
  if (!r) return [];
  return Array.isArray(r) ? r : [r];
};

export const pickPresetsFile = () =>
  pick({ title: "Import presets", filters: [{ name: "Presets", extensions: ["json"] }, { name: "All files", extensions: ["*"] }] });

export const savePresetsFile = async (): Promise<string | null> =>
  (await save({ title: "Export presets", defaultPath: "comfywebm-presets.json", filters: [{ name: "Presets", extensions: ["json"] }] })) ?? null;
