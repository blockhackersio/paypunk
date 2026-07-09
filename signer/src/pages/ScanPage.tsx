import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Page, Navbar, Block, BlockTitle, Button, Preloader } from "konsta/react";
import { invoke, isTauri } from "../backend";

export default function ScanPage() {
  const navigate = useNavigate();
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleScan = async () => {
    setError(null);
    setScanning(true);
    try {
      let content: string;

      if (isTauri()) {
        const { scan, Format, requestPermissions, checkPermissions, openAppSettings } = await import("@tauri-apps/plugin-barcode-scanner");

        let perm = await checkPermissions();
        if (perm !== "granted") {
          perm = await requestPermissions();
        }
        if (perm !== "granted") {
          setError("Camera permission denied. Please grant camera access in settings.");
          try {
            await openAppSettings();
          } catch {
            // openAppSettings may not be available on all platforms
          }
          setScanning(false);
          return;
        }

        const scanned = await scan({ windowed: false, formats: [Format.QRCode] });
        content = scanned.content;
      } else {
        content = prompt("Browser mock: paste hex QR content (or leave empty for demo data)") || "";
        if (!content) {
          content = "00"; // minimal mock payload
        }
      }

      const result = await invoke<string>("process_scanned_qr", { qr_data: content });
      if (result) {
        navigate("/preview");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("Scan failed:", msg);
      setError(msg);
    } finally {
      setScanning(false);
    }
  };

  return (
    <Page>
      <Navbar title="Scan QR" />
      <BlockTitle>Scan Transaction</BlockTitle>
      <Block strong className="text-center">
        <p className="mb-4 text-gray-500">
          Scan a QR code from the PayPunk Bridge to preview and sign a transaction.
        </p>
        <Button large rounded className="w-full" onClick={handleScan} disabled={scanning}>
          {scanning ? "Scanning..." : "Scan QR Code"}
        </Button>
        {scanning && (
          <div className="flex justify-center mt-4">
            <Preloader />
          </div>
        )}
      </Block>
      {error && (
        <Block strong className="text-center">
          <p className="text-red-500">{error}</p>
          <Button className="mt-2" onClick={() => setError(null)}>Dismiss</Button>
        </Block>
      )}
    </Page>
  );
}
