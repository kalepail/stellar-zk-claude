import { AsteroidsCanvas } from "./components/AsteroidsCanvas";
import "./App.css";

function App() {
  return (
    <main className="app-shell">
      <section className="headline">
        <h1>Asteroids Clone</h1>
        <p>
          Built for fast, deterministic gameplay with clean architecture and easy extension points.
        </p>
      </section>

      <section className="game-panel" aria-label="Asteroids game panel">
        <AsteroidsCanvas />
      </section>

      <section className="footnote">
        <p>
          Controls: <strong>Arrow Keys</strong> move and turn, <strong>Space</strong> fires,
          <strong> Shift</strong> hyperspaces, <strong>P</strong> pauses, <strong>R</strong>{" "}
          restarts, <strong>Esc</strong> quits to menu.
        </p>
      </section>
    </main>
  );
}

export default App;
