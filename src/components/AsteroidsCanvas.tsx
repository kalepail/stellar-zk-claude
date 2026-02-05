import { useEffect, useRef } from "react";
import { AsteroidsGame } from "../game/AsteroidsGame";

export function AsteroidsCanvas() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;

    if (!canvas) {
      return;
    }

    const game = new AsteroidsGame(canvas);

    return () => {
      game.dispose();
    };
  }, []);

  return <canvas ref={canvasRef} className="asteroids-canvas" />;
}
