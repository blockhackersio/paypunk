export interface ArtifactSummary {
  outputs: Array<{ address: string; amount: string }>;
  fee: string;
}

export interface ZcashPreview {
  Zcash: ArtifactSummary;
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

// ── Mock implementations ──────────────────────────────────────────

let mockState: {
  status: "idle" | "previewing" | "signed";
  mnemonic: string;
  preview: ZcashPreview | null;
  signedHex: string;
} = {
  status: "idle",
  mnemonic: "ribbon velvet ocean puzzle harvest guitar shadow ladder comfort raven spring anchor",
  preview: null,
  signedHex: "",
};

async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  switch (cmd) {
    case "generate_seed": {
      mockState.mnemonic = "mock seed phrase generated for browser testing";
      mockState.status = "idle";
      return mockState.mnemonic as T;
    }

    case "get_signer_status": {
      return mockState.status as T;
    }

    case "process_scanned_qr": {
      const qrData = args?.qr_data as string;
      if (!qrData) throw new Error("no qr_data provided");
      // In mock mode, simulate a preview artifact response
      mockState.preview = {
        Zcash: {
          outputs: [
            { address: "zs1mock...", amount: "10000" },
          ],
          fee: "1000",
        },
      };
      mockState.status = "previewing";
      return "00" as T;
    }

    case "approve_and_sign": {
      mockState.signedHex = "deadbeef";
      mockState.status = "signed";
      return mockState.signedHex as T;
    }

    case "get_preview": {
      if (mockState.preview) {
        return mockState.preview as T;
      }
      throw new Error("no preview available");
    }

    default:
      throw new Error(`Unknown mock command: ${cmd}`);
  }
}

export { isTauri, invoke };
