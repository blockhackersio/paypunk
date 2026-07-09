import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import OnboardingPage from "./pages/OnboardingPage";
import ScanPage from "./pages/ScanPage";
import PreviewPage from "./pages/PreviewPage";
import SigningPage from "./pages/SigningPage";
import ResultPage from "./pages/ResultPage";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<OnboardingPage />} />
        <Route path="/scan" element={<ScanPage />} />
        <Route path="/preview" element={<PreviewPage />} />
        <Route path="/signing" element={<SigningPage />} />
        <Route path="/result" element={<ResultPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
