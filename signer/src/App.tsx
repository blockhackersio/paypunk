import { useState, useEffect } from "react";
import {
  App as KonstaApp,
  Page,
} from "konsta/react";
import ScanPage from "./pages/ScanPage";
import { invoke, Settings } from "./backend";

export default function App() {
  const [theme, setTheme] = useState<"ios" | "material">("material");

  useEffect(() => {
    (async () => {
      try {
        const settings = await invoke<Settings>("get_settings");
        setTheme(settings.theme_preference as "ios" | "material");
      } catch {
        // fallback
      }
    })();
  }, []);

  return (
    <KonstaApp theme={theme}>
      <div className="flex flex-col min-h-screen">
        <Page className="relative flex-1 overflow-auto">
          <ScanPage />
        </Page>
      </div>
    </KonstaApp>
  );
}