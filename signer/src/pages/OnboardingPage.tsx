import { useState } from "react";
import { useNav } from "../nav";
import { Page, Navbar, Block, BlockTitle, Button, Preloader } from "konsta/react";
import { invoke } from "../backend";

export default function OnboardingPage() {
  const { navigate } = useNav();
  const [generating, setGenerating] = useState(false);
  const [mnemonic, setMnemonic] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleGenerate = async () => {
    setGenerating(true);
    setError(null);
    try {
      const result = await invoke<string>("generate_seed");
      setMnemonic(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setGenerating(false);
    }
  };

  return (
    <Page>
      <Navbar title="PayPunk Signer" />
      <BlockTitle>Welcome</BlockTitle>
      <Block strong className="text-center">
        <p className="mb-4 text-gray-500">
          This app holds your seed phrase and signs transactions.
          Generate a seed to get started.
        </p>
        <div style={{ display: mnemonic ? "none" : "block" }}>
          <Button large rounded className="w-full" onClick={handleGenerate} disabled={generating}>
            {generating ? "Generating..." : "Generate Seed"}
          </Button>
        </div>
        <div style={{ display: mnemonic ? "block" : "none" }}>
          <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4 mb-4 text-sm font-mono break-all">
            {mnemonic}
          </div>
          <Button large rounded className="w-full" onClick={() => navigate("/scan")}>
            Continue to Scan
          </Button>
        </div>
        <div className="flex justify-center mt-4" style={{ display: generating ? "flex" : "none" }}>
          <Preloader />
        </div>
      </Block>
      <Block strong className="text-center" style={{ display: error ? "block" : "none" }}>
        <p className="text-red-500">{error}</p>
      </Block>
    </Page>
  );
}