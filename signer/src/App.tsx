import { useState, useEffect, useCallback } from "react";
import {
  App as KonstaApp,
  Page,
  Navbar,
  Toolbar,
  TabbarLink,
} from "konsta/react";
import InputsPage from "./pages/InputsPage";
import ListsPage from "./pages/ListsPage";
import OverlaysPage from "./pages/OverlaysPage";
import FeedbackPage from "./pages/FeedbackPage";
import SettingsPage from "./pages/SettingsPage";
import { invoke, Settings } from "./backend";

const TABS = [
  { label: "Inputs", icon: `<svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M4 6h16v2H4V6zm0 5h16v2H4v-2zm0 5h16v2H4v-2z"/></svg>` },
  { label: "Lists", icon: `<svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M3 4h18v2H3V4zm0 7h18v2H3v-2zm0 7h18v2H3v-2z"/></svg>` },
  { label: "Overlays", icon: `<svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M5 3h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2zm0 2v14h14V5H5z"/></svg>` },
  { label: "Feedback", icon: `<svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/></svg>` },
  { label: "Settings", icon: `<svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.488.488 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1115.6 12 3.611 3.611 0 0112 15.6z"/></svg>` },
];

export default function App() {
  const [activeTab, setActiveTab] = useState(0);
  const [theme, setTheme] = useState<"ios" | "material">("material");
  const [launchCount, setLaunchCount] = useState(0);

  useEffect(() => {
    (async () => {
      try {
        const settings = await invoke<Settings>("get_settings");
        setTheme(settings.theme_preference as "ios" | "material");
        setLaunchCount(settings.launch_count);
      } catch {
        // fallback
      }
    })();
  }, []);

  const handleThemeChange = useCallback(async (newTheme: "ios" | "material") => {
    setTheme(newTheme);
    try {
      await invoke("save_settings", { theme_preference: newTheme });
    } catch {
      // mock fallback
    }
  }, []);

  const renderPage = () => {
    switch (activeTab) {
      case 0: return <InputsPage />;
      case 1: return <ListsPage />;
      case 2: return <OverlaysPage />;
      case 3: return <FeedbackPage />;
      case 4: return <SettingsPage theme={theme} onThemeChange={handleThemeChange} launchCount={launchCount} />;
      default: return null;
    }
  };

  return (
    <KonstaApp theme={theme}>
      <div className="flex flex-col min-h-screen">
        <Page className="relative flex-1 overflow-auto">
          <Navbar title="PayPunk Kitchen Sink" />
          {renderPage()}
        </Page>
        <Toolbar tabbar>
          {TABS.map((tab, idx) => (
            <TabbarLink
              key={tab.label}
              active={activeTab === idx}
              label={tab.label}
              onClick={() => setActiveTab(idx)}
            />
          ))}
        </Toolbar>
      </div>
    </KonstaApp>
  );
}
