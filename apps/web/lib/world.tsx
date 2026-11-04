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
import { useTranslation } from "react-i18next";
import { useDownloader } from "@/lib/downloader";
import { TranslatedError, UserFacingError } from "@/lib/errors";
import {
  openSave,
  parseEntities,
  startSaveWorld,
  startWorld,
  type CameraSpec,
  type EntityInfo,
  type ReplayHandle,
  type SaveHandle,
  type WorldHandle,
  startReplayWorld,
} from "@/lib/renderer";

export const DEFAULT_SEED = BigInt(8675309);
export const DEFAULT_CAMERA: CameraSpec = {
  position: [0, 90, 0],
  yaw: 0,
  pitch: 20,
};

export type WorldMode = "world" | "save" | "replay";

export interface WorldContextValue {
  seed: bigint;
  setSeed: (seed: bigint) => void;
  camera: CameraSpec;
  setCamera: (camera: CameraSpec) => void;
  generate: () => Promise<void>;
  load: (path: string) => Promise<void>;
  isLoading: boolean;
  mode: WorldMode;
  entities: EntityInfo[];
  players: EntityInfo[];
  attachCanvas: (canvas: HTMLCanvasElement | null) => void;
  showReplay: (replay: ReplayHandle) => Promise<void>;
  showWorld: () => Promise<void>;
  publishEntities: (entities: EntityInfo[]) => void;
  applyCamera: (camera: CameraSpec) => void;
}

export interface CameraContextValue {
  followed: EntityInfo | null;
  follow: (entity: EntityInfo) => void;
  unfollow: () => void;
  nextPlayer: () => void;
  prevPlayer: () => void;
}

const WorldContext = createContext<WorldContextValue | null>(null);
const CameraContext = createContext<CameraContextValue | null>(null);

function entityKey(entity: EntityInfo): string {
  return entity.uuid ?? `id:${entity.id}`;
}

function eyeCamera(entity: EntityInfo): CameraSpec | null {
  if (!entity.position) return null;
  const [x, y, z] = entity.position;
  return { position: [x, y + 1.62, z], yaw: entity.yaw, pitch: entity.pitch };
}

export function WorldProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const downloader = useDownloader();
  const [seed, setSeed] = useState<bigint>(DEFAULT_SEED);
  const [camera, setCameraState] = useState<CameraSpec>(DEFAULT_CAMERA);
  const [isLoading, setIsLoading] = useState(false);
  const [mode, setMode] = useState<WorldMode>("world");
  const [entities, setEntities] = useState<EntityInfo[]>([]);
  const [followedKey, setFollowedKey] = useState<string | null>(null);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const handleRef = useRef<WorldHandle | null>(null);
  const seedRef = useRef(seed);
  const cameraRef = useRef(camera);
  useEffect(() => {
    seedRef.current = seed;
  }, [seed]);
  useEffect(() => {
    cameraRef.current = camera;
  }, [camera]);
  const startedRef = useRef(false);

  const stopCurrent = useCallback(() => {
    handleRef.current?.stop();
    handleRef.current?.free();
    handleRef.current = null;
  }, []);

  const runChainRef = useRef<Promise<void>>(Promise.resolve());

  const run = useCallback(
    (
      start: (canvas: HTMLCanvasElement) => Promise<WorldHandle>,
      nextMode: WorldMode,
    ): Promise<void> => {
      const task = async () => {
        const canvas = canvasRef.current;
        if (!canvas) return;
        stopCurrent();
        setIsLoading(true);
        downloader.notifier.begin();
        try {
          const handle = await start(canvas);
          handleRef.current = handle;
          setMode(nextMode);
        } catch (error) {
          throw new UserFacingError(t("errors.startFailed"), error);
        } finally {
          downloader.notifier.end();
          setIsLoading(false);
        }
      };
      const result = runChainRef.current.then(task, task);
      runChainRef.current = result.then(
        () => {},
        () => {},
      );
      return result;
    },
    [stopCurrent, t, downloader],
  );

  const generate = useCallback(async () => {
    setEntities([]);
    setFollowedKey(null);
    await run(
      (canvas) =>
        startWorld(downloader, canvas, {
          seed: seedRef.current,
          camera: cameraRef.current,
        }),
      "world",
    );
  }, [run, downloader]);

  const load = useCallback(
    async (path: string) => {
      const response = await fetch(downloader.config.publicUrl(path));
      if (!response.ok) {
        throw new TranslatedError("errors.fetchFailed", {
          path,
          status: response.status,
        });
      }
      const bytes = new Uint8Array(await response.arrayBuffer());
      const save: SaveHandle = await openSave(downloader, bytes);
      const players = parseEntities(save.players_json());
      const saveSeed = save.seed();
      await run(
        (canvas) =>
          startSaveWorld(downloader, canvas, save, {
            camera: cameraRef.current,
          }),
        "save",
      );
      setSeed(saveSeed);
      setEntities(players);
      setFollowedKey(null);
    },
    [run, downloader],
  );

  const showReplay = useCallback(
    async (replay: ReplayHandle) => {
      await run(
        (canvas) =>
          startReplayWorld(downloader, canvas, replay, {
            seed: seedRef.current,
            camera: cameraRef.current,
          }),
        "replay",
      );
    },
    [run, downloader],
  );

  const showWorld = useCallback(async () => {
    await generate();
  }, [generate]);

  const attachCanvas = useCallback(
    (canvas: HTMLCanvasElement | null) => {
      canvasRef.current = canvas;
      if (canvas && !startedRef.current) {
        startedRef.current = true;
        void generate();
      }
      if (!canvas) {
        stopCurrent();
        startedRef.current = false;
      }
    },
    [generate, stopCurrent],
  );

  const applyCamera = useCallback((next: CameraSpec) => {
    cameraRef.current = next;
    setCameraState(next);
    handleRef.current?.set_camera(
      next.position[0],
      next.position[1],
      next.position[2],
      next.yaw,
      next.pitch,
    );
  }, []);

  const setCamera = useCallback(
    (next: CameraSpec) => applyCamera(next),
    [applyCamera],
  );

  const publishEntities = useCallback((next: EntityInfo[]) => {
    setEntities(next);
  }, []);

  const players = useMemo(
    () => entities.filter((entity) => entity.isPlayer),
    [entities],
  );

  const followed = useMemo(
    () =>
      followedKey
        ? (entities.find((entity) => entityKey(entity) === followedKey) ??
          null)
        : null,
    [entities, followedKey],
  );

  useEffect(() => {
    if (!followed) return;
    const eye = eyeCamera(followed);
    if (eye) {
      handleRef.current?.set_camera(
        eye.position[0],
        eye.position[1],
        eye.position[2],
        eye.yaw,
        eye.pitch,
      );
    }
  }, [followed]);

  const follow = useCallback((entity: EntityInfo) => {
    setFollowedKey(entityKey(entity));
    const eye = eyeCamera(entity);
    if (eye) applyCamera(eye);
  }, [applyCamera]);

  const cyclePlayer = useCallback(
    (step: 1 | -1) => {
      if (players.length === 0) return;
      const index = followedKey
        ? players.findIndex((entity) => entityKey(entity) === followedKey)
        : -1;
      const next =
        players[(index + step + players.length) % players.length];
      follow(next);
    },
    [players, followedKey, follow],
  );

  const worldValue = useMemo<WorldContextValue>(
    () => ({
      seed,
      setSeed,
      camera,
      setCamera,
      generate,
      load,
      isLoading,
      mode,
      entities,
      players,
      attachCanvas,
      showReplay,
      showWorld,
      publishEntities,
      applyCamera,
    }),
    [
      seed,
      camera,
      setCamera,
      generate,
      load,
      isLoading,
      mode,
      entities,
      players,
      attachCanvas,
      showReplay,
      showWorld,
      publishEntities,
      applyCamera,
    ],
  );

  const cameraValue = useMemo<CameraContextValue>(
    () => ({
      followed,
      follow,
      unfollow: () => setFollowedKey(null),
      nextPlayer: () => cyclePlayer(1),
      prevPlayer: () => cyclePlayer(-1),
    }),
    [followed, follow, cyclePlayer],
  );

  return (
    <WorldContext.Provider value={worldValue}>
      <CameraContext.Provider value={cameraValue}>
        {children}
      </CameraContext.Provider>
    </WorldContext.Provider>
  );
}

export function useWorld(): WorldContextValue {
  const value = useContext(WorldContext);
  if (!value) throw new Error("useWorld outside WorldProvider");
  return value;
}

export function useEntity(): { entities: EntityInfo[]; players: EntityInfo[] } {
  const { entities, players } = useWorld();
  return { entities, players };
}

export function useCamera(): CameraContextValue {
  const value = useContext(CameraContext);
  if (!value) throw new Error("useCamera outside WorldProvider");
  return value;
}
