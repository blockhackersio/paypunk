import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Page, Navbar, Block, BlockTitle, Button, List, ListItem, Preloader } from "konsta/react";
import { invoke } from "../backend";

interface OutputEntry {
  address: string;
  amount: string;
}

interface ZcashArtifactSummary {
  outputs: OutputEntry[];
  fee: string;
}

interface ArtifactSummary {
  Zcash?: ZcashArtifactSummary;
}

export default function PreviewPage() {
  const navigate = useNavigate();
  const [preview, setPreview] = useState<ArtifactSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [signing, setSigning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const data = await invoke<ArtifactSummary>("get_preview");
        setPreview(data);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const handleApprove = async () => {
    setSigning(true);
    setError(null);
    try {
      await invoke<string>("approve_and_sign");
      navigate("/signing");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSigning(false);
    }
  };

  const handleReject = () => {
    navigate("/scan");
  };

  if (loading) {
    return (
      <Page>
        <Navbar title="Preview" />
        <Block strong className="text-center">
          <Preloader />
          <p className="mt-4 text-gray-500">Loading preview...</p>
        </Block>
      </Page>
    );
  }

  const zcashPreview = preview?.Zcash;

  return (
    <Page>
      <Navbar title="Transaction Preview" />
      <BlockTitle>Transaction Details</BlockTitle>
      {zcashPreview ? (
        <>
          <Block strong>
            <List>
              <ListItem title="Outputs" after={String(zcashPreview.outputs.length)} />
              {zcashPreview.outputs.map((out, i) => (
                <ListItem
                  key={i}
                  title={`Output ${i + 1}`}
                  subtitle={`${out.amount} zatoshis`}
                  after={out.address.slice(0, 12) + "..."}
                />
              ))}
              <ListItem title="Fee" after={`${zcashPreview.fee} zatoshis`} />
            </List>
          </Block>
          <Block strong className="text-center">
            <Button large rounded className="w-full mb-2" onClick={handleApprove} disabled={signing}>
              {signing ? "Signing..." : "Approve & Sign"}
            </Button>
            <Button large rounded outline className="w-full" onClick={handleReject} disabled={signing}>
              Reject
            </Button>
          </Block>
        </>
      ) : (
        <Block strong className="text-center">
          <p className="text-gray-500">No preview data available.</p>
          <Button className="mt-4" onClick={() => navigate("/scan")}>Back to Scan</Button>
        </Block>
      )}
      {signing && (
        <Block strong className="text-center">
          <Preloader />
        </Block>
      )}
      {error && (
        <Block strong className="text-center">
          <p className="text-red-500">{error}</p>
        </Block>
      )}
    </Page>
  );
}
