export interface AppInfo {
  app_name: string;
  app_version: string;
  target_triple: string;
  build_profile: string;
  source: "rust" | "mock";
}

export interface GreetResult {
  message: string;
}

export interface ListItem {
  id: number;
  title: string;
  description: string;
  category: string;
}

export interface Settings {
  theme_preference: string;
  launch_count: number;
  favourite_color: string;
  note: string;
}

export interface TimerTick {
  tick: number;
}

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mockInvoke<T>(cmd, args);
}

async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (e) => handler(e.payload));
  }
  return mockListen(event, handler);
}

// ── Mock implementations ──────────────────────────────────────────

let mockSettings: Settings = {
  theme_preference: "material",
  launch_count: 0,
  favourite_color: "#ff0000",
  note: "Browser mock — data is not persisted",
};

let mockTimerRunning = false;
const mockListeners: Record<string, Array<(payload: unknown) => void>> = {};

async function mockInvoke<T>(cmd: string, _args?: Record<string, unknown>): Promise<T> {
  switch (cmd) {
    case "get_app_info":
      return {
        app_name: "PayPunk Kitchen Sink",
        app_version: "0.1.0",
        target_triple: "mock-x86_64-unknown-linux-gnu",
        build_profile: "mock",
        source: "mock",
      } as T;

    case "greet":
      return { message: `Hello, ${_args?.name ?? "stranger"}! (mock)` } as T;

    case "get_list_items":
      return [
        { id: 1, title: "Mock Item Alpha", description: "This is a mock item from the browser fallback", category: "mock" },
        { id: 2, title: "Mock Item Beta", description: "Another mock item for demonstration", category: "mock" },
        { id: 3, title: "Mock Item Gamma", description: "Yet another mock item", category: "mock" },
      ] as T;

    case "get_settings":
      return { ...mockSettings } as T;

    case "save_settings":
      mockSettings = { ...mockSettings, ...(_args as Record<string, unknown>) as unknown as Partial<Settings> };
      return { ...mockSettings } as T;

    default:
      throw new Error(`Unknown mock command: ${cmd}`);
  }
}

async function mockListen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!mockListeners[event]) mockListeners[event] = [];
  mockListeners[event].push(handler as (payload: unknown) => void);

  if (event === "timer-tick" && !mockTimerRunning) {
    mockTimerRunning = true;
    let tick = 0;
    const interval = setInterval(() => {
      tick++;
      (mockListeners["timer-tick"] ?? []).forEach((h) => h({ tick }));
    }, 1000);
    return () => {
      clearInterval(interval);
      mockTimerRunning = false;
    };
  }

  return () => {
    const idx = mockListeners[event]?.indexOf(handler as (payload: unknown) => void) ?? -1;
    if (idx >= 0) mockListeners[event]?.splice(idx, 1);
  };
}

export { isTauri, invoke, listen };
