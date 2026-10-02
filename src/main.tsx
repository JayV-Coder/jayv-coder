import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import { lockDownWebview } from "@/modules/lockdown";
import { connectTheme } from "@/modules/theme";
import "@fontsource-variable/inter";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "@/styles/globals.css";

// O tema entra antes da primeira pintura: sem isso a tela piscaria clara.
connectTheme();
// No build de produção a janela fica sem inspetor, código-fonte e recarga.
lockDownWebview();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
