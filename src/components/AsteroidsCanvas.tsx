import { useEffect, useRef } from "react";
import type { GameRunRecord } from "../game/AsteroidsGame";
import { AsteroidsGame } from "../game/AsteroidsGame";

export interface CompletedGameRun {
  record: GameRunRecord;
  frameCount: number;
  endedAtMs: number;
  claimantLock: string | null;
}

interface AsteroidsCanvasProps {
  onGameOver?: (run: CompletedGameRun) => void;
}

export function AsteroidsCanvas({ onGameOver }: AsteroidsCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const onGameOverRef = useRef(onGameOver);

  useEffect(() => {
    onGameOverRef.current = onGameOver;
  }, [onGameOver]);

  useEffect(() => {
    const canvas = canvasRef.current;

    if (!canvas) {
      return;
    }

    const game = new AsteroidsGame({ canvas });
    let modeBefore = game.getMode();
    let watcherFrame: number | null = null;
    let disposed = false;

    const watchModeTransitions = () => {
      if (disposed) {
        return;
      }

      const modeNow = game.getMode();
      if (modeNow === "game-over" && modeBefore !== "game-over") {
        const record = game.getRunRecord();
        if (record) {
          onGameOverRef.current?.({
            record,
            frameCount: record.inputs.length,
            endedAtMs: Date.now(),
            claimantLock: null,
          });
        }
      }

      modeBefore = modeNow;
      watcherFrame = window.requestAnimationFrame(watchModeTransitions);
    };

    watcherFrame = window.requestAnimationFrame(watchModeTransitions);

    return () => {
      disposed = true;
      if (watcherFrame !== null) {
        window.cancelAnimationFrame(watcherFrame);
      }
      game.dispose();
    };
  }, []);

  return <canvas ref={canvasRef} className="asteroids-canvas" />;
}
