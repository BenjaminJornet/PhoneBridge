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
          <svg aria-label="PhoneBridge" className="brandIcon" role="img" viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg">
            <defs>
              <linearGradient gradientUnits="userSpaceOnUse" id="brand-bg" x1="160" x2="864" y1="96" y2="928">
                <stop offset="0" stopColor="#0f766e" />
                <stop offset="0.52" stopColor="#2563eb" />
                <stop offset="1" stopColor="#4f46e5" />
              </linearGradient>
              <linearGradient gradientUnits="userSpaceOnUse" id="brand-phone" x1="330" x2="694" y1="190" y2="834">
                <stop offset="0" stopColor="#f8fafc" />
                <stop offset="1" stopColor="#cbd5e1" />
              </linearGradient>
              <filter id="brand-shadow" x="-20%" y="-20%" width="140%" height="140%">
                <feDropShadow dx="0" dy="28" floodColor="#020617" floodOpacity="0.35" stdDeviation="30" />
              </filter>
            </defs>
            <rect fill="url(#brand-bg)" height="1024" rx="232" width="1024" />
            <path d="M184 596c72 132 191 208 328 208 139 0 259-78 330-214" fill="none" opacity="0.95" stroke="#99f6e4" strokeLinecap="round" strokeWidth="54" />
            <path d="M808 492l56 110-121 28" fill="none" opacity="0.95" stroke="#99f6e4" strokeLinecap="round" strokeLinejoin="round" strokeWidth="54" />
            <path d="M840 428C768 296 649 220 512 220c-139 0-259 78-330 214" fill="none" opacity="0.9" stroke="#bfdbfe" strokeLinecap="round" strokeWidth="54" />
            <path d="M216 532l-56-110 121-28" fill="none" opacity="0.9" stroke="#bfdbfe" strokeLinecap="round" strokeLinejoin="round" strokeWidth="54" />
            <g filter="url(#brand-shadow)">
              <rect fill="#0f172a" height="672" rx="72" width="336" x="344" y="176" />
              <rect fill="url(#brand-phone)" height="544" rx="38" width="256" x="384" y="228" />
              <rect fill="#64748b" height="18" rx="9" width="116" x="454" y="198" />
              <circle cx="512" cy="804" fill="#64748b" r="22" />
              <path d="M448 506h128m-64-64v128" stroke="#2563eb" strokeLinecap="round" strokeWidth="42" />
            </g>
          </svg>
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
