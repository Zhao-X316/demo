import { convertFileSrc } from "@tauri-apps/api/core";

/** 本地音频回放：经 Tauri asset 协议加载文件路径。 */
export function AudioPlayer({ path }: { path: string }) {
  if (!path) return null;
  return <audio className="audioplayer" controls preload="none" src={convertFileSrc(path)} />;
}
