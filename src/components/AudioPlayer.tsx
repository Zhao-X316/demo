import { convertFileSrc } from "@tauri-apps/api/core";

/** 本地音频回放：经 Tauri asset 协议加载文件路径，并把可读状态交给终审闸门。 */
export function AudioPlayer({
  path,
  onReady,
  onError,
}: {
  path: string;
  onReady?: () => void;
  onError?: () => void;
}) {
  if (!path) return <div className="evidence-error">未找到录音文件路径</div>;
  return (
    <audio
      className="audioplayer"
      controls
      preload="metadata"
      src={convertFileSrc(path)}
      onLoadedMetadata={onReady}
      onCanPlay={onReady}
      onError={onError}
    />
  );
}
