import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/monaspace-krypton/400.css";
import "@fontsource/monaspace-krypton/500.css";
import "@fontsource/monaspace-krypton/600.css";
import "@fontsource/monaspace-krypton/700.css";
import "@fontsource/monaspace-neon/600.css";
import "@fontsource/monaspace-neon/700.css";
import App from "./App.tsx";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
