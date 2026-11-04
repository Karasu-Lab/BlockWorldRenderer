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
import { UserFacingError } from "@/lib/errors";
import {
  openSave,
  parseEntities,
  startSaveWorld,
  startWorld,
  type CameraSpec,
  type EntityInfo,
  type LoadProgress,
  type ReplayHandle,
  type SaveHandle,
  type WorldHandle,
  startReplayWorld,
  publicUrl,
} from "@/lib/renderer";

/** The seed and camera the generated view starts from. */
export const DEFAULT_SEED = BigInt(8675309);
export const DEFAULT_CAMERA: CameraSpec = {
  position: [0, 90, 0],
  yaw: 0,
  pitch: 20,
};

/** What the canvas is currently showing. */
export type WorldMode = "world" | "save" | "replay";

export interface WorldContextValue {
  /** The seed `generate` builds the world from. */
  seed: bigint;
  setSeed: (seed: bigint) => void;
  /** The camera `generate` starts at, and `setCamera` moves live. */
  camera: CameraSpec;
  setCamera: (camera: CameraSpec) => void;
  /** (Re)starts the generated world with the current seed and camera. */
  generate: () => Promise<void>;
  /**
   * Loads a world save: fetches the zip at `path`, reads its seed and
   * players, and shows it. Its players become the entity list.
   */
  load: (path: string) => Promise<void>;
  isLoading: boolean;
  progress: LoadProgress | null;
  mode: WorldMode;
  /** The entities of the current replay or save. */
  entities: EntityInfo[];
  players: EntityInfo[];
  // Session plumbing the replay provider and the canvas build on.
  attachCanvas: (canvas: HTMLCanvasElement | null) => void;
  showReplay: (replay: ReplayHandle) => Promise<void>;
  showWorld: () => Promise<void>;
  publishEntities: (entities: EntityInfo[]) => void;
  applyCamera: (camera: CameraSpec) => void;
}

export interface CameraContextValue {
  /** The player entity the camera is following, if any. */
  followed: EntityInfo | null;
  /** Follows an entity (a player, for the viewpoint to make sense). */
  follow: (entity: EntityInfo) => void;
  /** Stops following; the camera stays where it is. */
  unfollow: () => void;
  /** Follows the next player in the entity list, wrapping around. */
  nextPlayer: () => void;
  /** Follows the previous player in the entity list, wrapping around. */
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
  const [seed, setSeed] = useState<bigint>(DEFAULT_SEED);
  const [camera, setCameraState] = useState<CameraSpec>(DEFAULT_CAMERA);
  const [isLoading, setIsLoading] = useState(false);
  const [progress, setProgress] = useState<LoadProgress | null>(null);
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
        setProgress({
          available: false,
          fraction: 0,
          status: "",
          fileFraction: null,
          wasmVersion: null,
        });
        try {
          const handle = await start(canvas);
          handleRef.current = handle;
          setMode(nextMode);
        } catch (error) {
          throw new UserFacingError(t("errors.startFailed"), error);
        } finally {
          setIsLoading(false);
          setProgress(null);
        }
      };
      // Starts run one at a time: a start that begins while another
      // is still loading would download alongside it, and the two
      // would fight over the one shared progress state.
      const result = runChainRef.current.then(task, task);
      runChainRef.current = result.then(
        () => {},
        () => {},
      );
      return result;
    },
    [stopCurrent, t],
  );

  const generate = useCallback(async () => {
    setEntities([]);
    setFollowedKey(null);
    await run(
      (canvas) =>
        startWorld(canvas, {
          seed: seedRef.current,
          camera: cameraRef.current,
          onProgress: setProgress,
        }),
      "world",
    );
  }, [run]);

  const load = useCallback(
    async (path: string) => {
      const response = await fetch(publicUrl(path));
      if (!response.ok) {
        throw new Error(`cannot fetch ${path}: ${response.status}`);
      }
      const bytes = new Uint8Array(await response.arrayBuffer());
      const save: SaveHandle = await openSave(bytes);
      const players = parseEntities(save.players_json());
      const saveSeed = save.seed();
      await run(
        (canvas) =>
          startSaveWorld(canvas, save, {
            camera: cameraRef.current,
            onProgress: setProgress,
          }),
        "save",
      );
      setSeed(saveSeed);
      setEntities(players);
      setFollowedKey(null);
    },
    [run],
  );

  const showReplay = useCallback(
    async (replay: ReplayHandle) => {
      await run(
        (canvas) =>
          startReplayWorld(canvas, replay, {
            seed: seedRef.current,
            camera: cameraRef.current,
            onProgress: setProgress,
          }),
        "replay",
      );
    },
    [run],
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

  // While a player is followed, the camera tracks their viewpoint
  // whenever the entity list moves them.
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
      progress,
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
      progress,
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
