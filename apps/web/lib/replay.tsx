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
import {
  openReplay,
  parseEntities,
  replayUrl,
  type ReplayHandle,
} from "@/lib/renderer";
import { useWorld } from "@/lib/world";

export interface ReplayControls {
  /**
   * Loads the replay into the viewer. When the file does not exist,
   * nothing is loaded and the generated world stays on screen.
   */
  load: () => Promise<void>;
  /**
   * Moves the playhead; backward seeks replay from the start. The
   * viewer restarts on the new position, so the columns on screen
   * are the ones the recording had sent by then.
   */
  seek: (timeMs: number) => void;
  /** The current playhead position, in milliseconds. */
  time: number;
  /** The recording's total length, in milliseconds (0 before load). */
  duration: number;
  play: () => void;
  pause: () => void;
  /** Pauses and returns the playhead to the start. */
  stop: () => void;
  /** Unloads the replay and returns to the generated world. */
  close: () => Promise<void>;
  isPlaying: boolean;
  /** Whether the replay file exists and could be opened. */
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
        const response = await fetch(replayUrl(path));
        if (!response.ok) return; // No replay file: the world stays.
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (sessionRef.current?.path !== path) return;
        sessionRef.current.bytes = bytes;
        setAvailable(true);
      })();
    },
    [cancelLoop],
  );

  const load = useCallback(async () => {
    const session = sessionRef.current;
    if (!session || session.bytes.length === 0) return;
    cancelLoop();
    session.handle?.free();
    session.handle = await openReplay(session.bytes);
    await world.showReplay(session.handle);
    session.handle.apply_until(0);
    setDuration(session.handle.duration_ms());
    setTime(0);
    syncEntities();
  }, [cancelLoop, syncEntities, world]);

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
      // The viewer keeps the columns it already loaded, so the
      // viewer restarts on the moved playhead to show the state the
      // recording had reached by then. Restarts are chained: rapid
      // seeks queue up instead of racing, and the last one leaves
      // the viewer on the final state.
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

/**
 * Binds a replay file under `public/replays` and returns the controls
 * to operate it from anywhere in the tree.
 */
export function useReplay(path: string): ReplayControls {
  const context = useContext(ReplayContext);
  if (!context) throw new Error("useReplay outside ReplayProvider");
  const { attach, controls } = context;
  useEffect(() => {
    attach(path);
  }, [attach, path]);
  return controls;
}
