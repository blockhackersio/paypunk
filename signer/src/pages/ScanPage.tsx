import { useState } from "react";
import { Page, Navbar, Button, Block, BlockTitle, Popup, Preloader } from "konsta/react";
import { invoke, isTauri } from "../backend";

export default function ScanPage() {
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [qrSvg, setQrSvg] = useState<string | null>(null);
  const [popupOpened, setPopupOpened] = useState(false);

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
        // Browser mock: simulate scanning
        content = prompt("Browser mock: paste base64 QR content (or leave empty for demo data)") || "";
        if (!content) {
          // Provide a valid demo ping frame encoded as base64 for testing
          const frame = new Uint8Array([0x04, ...new TextEncoder().encode("ping"), ...new Uint8Array(32)]);
          content = btoa(String.fromCharCode(...frame));
        }
      }

      const result = await invoke<string>("process_scanned_qr", { content });
      setQrSvg(result);
      setPopupOpened(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("Scan failed:", msg);
      setError(msg);
    } finally {
      setScanning(false);
    }
  };

  const handleClosePopup = () => {
    setPopupOpened(false);
    setQrSvg(null);
  };

  return (
    <>
      <Navbar title="PayPunk Signer" />
      <BlockTitle>Scan & Sign</BlockTitle>
      <Block strong className="text-center">
        <p className="mb-4 text-gray-500">
          Scan a QR code from the PayPunk Bridge to sign a transaction.
        </p>
        <Button
          large
          rounded
          className="w-full"
          onClick={handleScan}
          disabled={scanning}
        >
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
          <Button className="mt-2" onClick={() => setError(null)}>
            Dismiss
          </Button>
        </Block>
      )}

      <Popup opened={popupOpened}>
        <Page>
          <Navbar title="Signed Response" />
          <Block strong className="text-center">
            <p className="mb-4 text-gray-500">
              Scan this QR code back at the bridge to complete the signing flow.
            </p>
            {qrSvg && (
              <div
                className="flex justify-center"
                dangerouslySetInnerHTML={{ __html: qrSvg }}
              />
            )}
            <Button className="mt-4" onClick={handleClosePopup}>
              Close
            </Button>
          </Block>
        </Page>
      </Popup>
    </>
  );
}