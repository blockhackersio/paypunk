import { useNavigate } from "react-router-dom";
import { Page, Navbar, Block, BlockTitle, Button } from "konsta/react";

export default function ResultPage() {
  const navigate = useNavigate();

  return (
    <Page>
      <Navbar title="Signed" />
      <BlockTitle>Transaction Signed</BlockTitle>
      <Block strong className="text-center">
        <p className="mb-4 text-gray-500">
          The transaction has been signed. Present this device back to the bridge
          to scan the response QR code and complete the flow.
        </p>
        <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-8 mb-4 flex justify-center">
          <div className="w-48 h-48 bg-white rounded flex items-center justify-center">
            <p className="text-gray-400 text-sm text-center">
              Response QR
              <br />
              (displayed here)
            </p>
          </div>
        </div>
        <Button large rounded className="w-full" onClick={() => navigate("/scan")}>
          Done
        </Button>
      </Block>
    </Page>
  );
}
