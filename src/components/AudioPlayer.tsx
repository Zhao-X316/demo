import { convertFileSrc } from "@tauri-apps/api/core";
import { forwardRef, useImperativeHandle, useRef } from "react";

export interface AudioPlayerHandle {
  playRange: (startMs: number, endMs: number) => Promise<void>;
}

interface AudioPlayerProps {
  path: string;
  onReady?: () => void;
  onError?: () => void;
  onRangeEnd?: () => void;
}

/** 本地音频回放：经 Tauri asset 协议加载文件路径，并把可读状态交给终审闸门。 */
export const AudioPlayer = forwardRef<AudioPlayerHandle, AudioPlayerProps>(function AudioPlayer(
  { path, onReady, onError, onRangeEnd },
  ref,
) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const rangeEndSeconds = useRef<number | null>(null);

  useImperativeHandle(ref, () => ({
    async playRange(startMs: number, endMs: number) {
      const audio = audioRef.current;
      if (!audio || audio.readyState === 0) {
        throw new Error("录音尚未准备好，请稍后再试");
      }
      if (
        !Number.isFinite(startMs) ||
        !Number.isFinite(endMs) ||
        startMs < 0 ||
        endMs <= startMs
      ) {
        throw new Error("该证据的时间片段无效，无法跳播");
      }
      const startSeconds = startMs / 1000;
      if (Number.isFinite(audio.duration) && startSeconds >= audio.duration) {
        throw new Error("该证据时间超出录音长度，无法跳播");
      }
      const durationEnd = Number.isFinite(audio.duration) ? audio.duration : endMs / 1000;
      const nextRangeEndSeconds = Math.min(endMs / 1000, durationEnd);
      rangeEndSeconds.current = null;
      if (Math.abs(audio.currentTime - startSeconds) > 0.05) {
        await new Promise<void>((resolve, reject) => {
          const timeout = window.setTimeout(() => {
            audio.removeEventListener("seeked", handleSeeked);
            reject(new Error("录音跳转超时，请手动拖动播放条核对"));
          }, 2_000);
          const handleSeeked = () => {
            window.clearTimeout(timeout);
            resolve();
          };
          audio.addEventListener("seeked", handleSeeked, { once: true });
          audio.currentTime = startSeconds;
        });
      }
      rangeEndSeconds.current = nextRangeEndSeconds;
      try {
        await audio.play();
      } catch (error) {
        rangeEndSeconds.current = null;
        throw error;
      }
    },
  }));

  if (!path) return <div className="evidence-error">未找到录音文件路径</div>;
  return (
    <audio
      ref={audioRef}
      className="audioplayer"
      controls
      preload="auto"
      src={convertFileSrc(path)}
      onCanPlay={onReady}
      onError={onError}
      onTimeUpdate={(event) => {
        const endSeconds = rangeEndSeconds.current;
        if (endSeconds !== null && event.currentTarget.currentTime >= endSeconds) {
          rangeEndSeconds.current = null;
          event.currentTarget.pause();
          onRangeEnd?.();
        }
      }}
      onPause={() => {
        if (rangeEndSeconds.current !== null) {
          rangeEndSeconds.current = null;
          onRangeEnd?.();
        }
      }}
      onEnded={() => {
        rangeEndSeconds.current = null;
        onRangeEnd?.();
      }}
    />
  );
});
