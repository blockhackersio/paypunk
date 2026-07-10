import { NavProvider, useNav } from "./nav";
import OnboardingPage from "./pages/OnboardingPage";
import ScanPage from "./pages/ScanPage";
import PreviewPage from "./pages/PreviewPage";
import SigningPage from "./pages/SigningPage";
import ResultPage from "./pages/ResultPage";

function CurrentPage() {
  const { page } = useNav();
  switch (page) {
    case "/":
      return <OnboardingPage />;
    case "/scan":
      return <ScanPage />;
    case "/preview":
      return <PreviewPage />;
    case "/signing":
      return <SigningPage />;
    case "/result":
      return <ResultPage />;
    default:
      return <OnboardingPage />;
  }
}

export default function App() {
  return (
    <NavProvider>
      <CurrentPage />
    </NavProvider>
  );
}
