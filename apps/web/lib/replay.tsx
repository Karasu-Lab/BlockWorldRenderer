"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useDownloader } from "@/lib/downloader";
import { openReplay, parseEntities, type ReplayHandle } from "@/lib/renderer";
import { useWorld } from "@/lib/world";

export interface ReplayControls {
  load: () => Promise<void>;
  seek: (timeMs: number) => void;
  time: number;
  duration: number;
  play: () => void;
  pause: () => void;
  stop: () => void;
  close: () => Promise<void>;
  isPlaying: boolean;
  available: boolean;
}

interface ReplaySession {
  path: string;
  bytes: Uint8Array;
  handle: ReplayHandle | null;
}

const ReplayContext = createContext<{
  attach: (path: string) => void;
  controls: ReplayControls;
} | null>(null);

export function ReplayProvider({ children }: { children: ReactNode }) {
  const world = useWorld();
  const downloader = useDownloader();
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [available, setAvailable] = useState(false);

  const sessionRef = useRef<ReplaySession | null>(null);
  const rafRef = useRef<number | null>(null);
  const lastStampRef = useRef<number | null>(null);

  const cancelLoop = useCallback(() => {
    if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
    lastStampRef.current = null;
    setIsPlaying(false);
  }, []);

  const syncEntities = useCallback(() => {
    const handle = sessionRef.current?.handle;
    if (handle) world.publishEntities(parseEntities(handle.entities_json()));
  }, [world]);

  const restartChainRef = useRef<Promise<void>>(Promise.resolve());

  const attach = useCallback(
    (path: string) => {
      if (sessionRef.current?.path === path) return;
      cancelLoop();
      sessionRef.current = { path, bytes: new Uint8Array(), handle: null };
      setTime(0);
      setDuration(0);
      setAvailable(false);
      void (async () => {
        const response = await fetch(downloader.config.replayUrl(path));
        if (!response.ok) return;
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (sessionRef.current?.path !== path) return;
        sessionRef.current.bytes = bytes;
        setAvailable(true);
      })();
    },
    [cancelLoop, downloader.config],
  );

  const load = useCallback(async () => {
    const session = sessionRef.current;
    if (!session || session.bytes.length === 0) return;
    cancelLoop();
    session.handle?.free();
    session.handle = await openReplay(downloader, session.bytes);
    await world.showReplay(session.handle);
    session.handle.apply_until(0);
    setDuration(session.handle.duration_ms());
    setTime(0);
    syncEntities();
  }, [cancelLoop, downloader, syncEntities, world]);

  const startPlayback = useCallback(() => {
    if (rafRef.current !== null) return;
    setIsPlaying(true);
    const step = (stamp: number) => {
      const last = lastStampRef.current ?? stamp;
      lastStampRef.current = stamp;
      const handle = sessionRef.current?.handle;
      if (!handle) return;
      const next = Math.min(
        handle.position_ms() + (stamp - last),
        handle.duration_ms(),
      );
      handle.apply_until(Math.round(next));
      setTime(Math.round(next));
      syncEntities();
      if (next >= handle.duration_ms()) {
        cancelLoop();
        return;
      }
      rafRef.current = requestAnimationFrame(step);
    };
    rafRef.current = requestAnimationFrame(step);
  }, [cancelLoop, syncEntities]);

  const seekTo = useCallback(
    (targetMs: number, resume: boolean) => {
      const session = sessionRef.current;
      if (!session?.handle) return;
      cancelLoop();
      const handle = session.handle;
      const clamped = Math.min(
        Math.max(0, Math.round(targetMs)),
        handle.duration_ms(),
      );
      handle.apply_until(clamped);
      setTime(clamped);
      syncEntities();
      restartChainRef.current = restartChainRef.current
        .catch(() => {})
        .then(() => world.showReplay(handle))
        .then(() => {
          if (resume) startPlayback();
        })
        .catch(() => {});
    },
    [cancelLoop, startPlayback, syncEntities, world],
  );

  const seek = useCallback(
    (timeMs: number) => seekTo(timeMs, rafRef.current !== null),
    [seekTo],
  );

  const play = useCallback(() => {
    if (sessionRef.current?.handle) {
      startPlayback();
    } else {
      void load().then(() => startPlayback());
    }
  }, [load, startPlayback]);

  const pause = useCallback(() => cancelLoop(), [cancelLoop]);

  const stop = useCallback(() => seekTo(0, false), [seekTo]);

  const close = useCallback(async () => {
    cancelLoop();
    const session = sessionRef.current;
    session?.handle?.free();
    if (session) session.handle = null;
    setTime(0);
    setDuration(0);
    world.publishEntities([]);
    await world.showWorld();
  }, [cancelLoop, world]);

  useEffect(() => cancelLoop, [cancelLoop]);

  const controls = useMemo<ReplayControls>(
    () => ({
      load,
      seek,
      time,
      duration,
      play,
      pause,
      stop,
      close,
      isPlaying,
      available,
    }),
    [load, seek, time, duration, play, pause, stop, close, isPlaying, available],
  );

  const value = useMemo(() => ({ attach, controls }), [attach, controls]);

  return (
    <ReplayContext.Provider value={value}>{children}</ReplayContext.Provider>
  );
}
export function useReplay(path: string): ReplayControls {
  const context = useContext(ReplayContext);
  if (!context) throw new Error("useReplay outside ReplayProvider");
  const { attach, controls } = context;
  useEffect(() => {
    attach(path);
  }, [attach, path]);
  return controls;
}
