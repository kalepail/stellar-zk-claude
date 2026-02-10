import { useEffect, useRef } from "react";
import { AsteroidsGame } from "../game/AsteroidsGame";

export interface CompletedGameRun {
  tape: Uint8Array;
  score: number;
  frameCount: number;
  seed: number;
  finalRngState: number;
  endedAtMs: number;
}

interface AsteroidsCanvasProps {
  onGameOver?: (run: CompletedGameRun) => void;
  claimantAddress?: string;
}

export function AsteroidsCanvas({ onGameOver, claimantAddress }: AsteroidsCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const onGameOverRef = useRef(onGameOver);
  const claimantAddressRef = useRef(claimantAddress ?? "");

  useEffect(() => {
    onGameOverRef.current = onGameOver;
  }, [onGameOver]);

  useEffect(() => {
    claimantAddressRef.current = claimantAddress ?? "";
  }, [claimantAddress]);

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
        const tape = game.getTape(claimantAddressRef.current);
        if (tape) {
          onGameOverRef.current?.({
            tape,
            score: game.getScore(),
            frameCount: game.getFrameCount(),
            seed: game.getGameSeed(),
            finalRngState: game.getRngState(),
            endedAtMs: Date.now(),
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
