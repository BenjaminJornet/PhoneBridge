import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Sync from "./pages/Sync";
import DataExplorer from "./pages/DataExplorer";

type Page = "dashboard" | "sync" | "gallery" | "data";
type Theme = "light" | "dark";

const icons: Record<Page, ReactNode> = {
  dashboard: (
    <svg aria-hidden="true" fill="none" height="20" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" viewBox="0 0 24 24" width="20">
      <path d="M3 11.5 12 4l9 7.5" />
      <path d="M5 10v10h14V10" />
    </svg>
  ),
  sync: (
    <svg aria-hidden="true" fill="none" height="20" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" viewBox="0 0 24 24" width="20">
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M5 21h14" />
    </svg>
  ),
  gallery: (
    <svg aria-hidden="true" fill="none" height="20" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" viewBox="0 0 24 24" width="20">
      <rect height="16" rx="2" width="18" x="3" y="4" />
      <circle cx="8.5" cy="9" r="1.5" />
      <path d="m21 16-5-5L5 20" />
    </svg>
  ),
  data: (
    <svg aria-hidden="true" fill="none" height="20" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" viewBox="0 0 24 24" width="20">
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5" />
      <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" />
    </svg>
  ),
};

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
              <span className="navIcon">{icons[item.id]}</span>
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
