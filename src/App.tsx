import { useEffect, useState } from "react";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Sync from "./pages/Sync";
import DataExplorer from "./pages/DataExplorer";

type Page = "dashboard" | "sync" | "gallery" | "data";
type Theme = "light" | "dark";

const pages: Array<{ id: Page; label: string }> = [
  { id: "dashboard", label: "Start" },
  { id: "sync", label: "Import" },
  { id: "gallery", label: "Library" },
  { id: "data", label: "Data tools" },
];

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window === "undefined") {
      return "light";
    }
    const stored = window.localStorage.getItem("phonebridge-theme");
    if (stored === "light" || stored === "dark") {
      return stored;
    }
    return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  });

  useEffect(() => {
    document.title = "PhoneBridge";
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem("phonebridge-theme", theme);
  }, [theme]);

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brandMark">PB</span>
          <div>
            <strong>PhoneBridge</strong>
            <small>Local Android data recovery</small>
          </div>
        </div>
        <nav>
          {pages.map((item) => (
            <button
              className={item.id === page ? "navItem active" : "navItem"}
              key={item.id}
              onClick={() => setPage(item.id)}
              type="button"
            >
              {item.label}
            </button>
          ))}
        </nav>
        <button
          className="themeToggle"
          onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
          type="button"
        >
          {theme === "dark" ? "Light mode" : "Dark mode"}
        </button>
      </aside>
      <main className="content">
        {page === "dashboard" && <Dashboard onNavigate={setPage} />}
        {page === "sync" && <Sync onNavigate={setPage} />}
        {page === "gallery" && <Gallery onImport={() => setPage("sync")} />}
        {page === "data" && <DataExplorer onNavigate={setPage} />}
      </main>
    </div>
  );
}
