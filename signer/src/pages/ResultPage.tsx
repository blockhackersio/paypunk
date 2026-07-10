import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Page, Navbar, Block, BlockTitle, Button } from "konsta/react";
import { invoke } from "../backend";

export default function ResultPage() {
  const navigate = useNavigate();
  const [qrSvg, setQrSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const responseB64 = await invoke<string>("get_response");
        const svg = await invoke<string>("generate_response_qr", { responseB64 });
        setQrSvg(svg);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
  }, []);

  return (
    <Page>
      <Navbar title="Signed" />
      <BlockTitle>Transaction Signed</BlockTitle>
      <Block strong className="text-center">
        <p className="mb-4 text-gray-500">
          The transaction has been signed. Present this device back to the bridge
          to scan the response QR code and complete the flow.
        </p>
        {error ? (
          <p className="text-red-500">{error}</p>
        ) : qrSvg ? (
          <div
            className="bg-white rounded-lg p-4 mb-4 flex justify-center inline-block"
            dangerouslySetInnerHTML={{ __html: qrSvg }}
          />
        ) : (
          <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-8 mb-4 flex justify-center">
            <div className="w-48 h-48 bg-white rounded flex items-center justify-center">
              <p className="text-gray-400 text-sm text-center">Loading QR...</p>
            </div>
          </div>
        )}
        <Button large rounded className="w-full" onClick={() => navigate("/scan")}>
          Done
        </Button>
      </Block>
    </Page>
  );
}
