import { createContext, useContext, useState, useCallback, ReactNode } from "react";

export type Page = "/" | "/scan" | "/preview" | "/signing" | "/result";

interface NavContextValue {
  page: Page;
  navigate: (to: Page) => void;
}

const NavContext = createContext<NavContextValue | null>(null);

export function NavProvider({ children }: { children: ReactNode }) {
  console.log('NavProvider render');
  const [page, setPage] = useState<Page>("/");
  const navigate = useCallback((to: Page) => {
    console.log("setPage called with :" + to);
    setPage(to)
  }, []);
  return (
    <NavContext.Provider value={{ page, navigate }}>
      {children}
    </NavContext.Provider>
  );
}

export function useNav(): NavContextValue {
  const ctx = useContext(NavContext);
  if (!ctx) throw new Error("useNav must be used within NavProvider");
  return ctx;
}
