import { useEffect, useState } from "react";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Sync from "./pages/Sync";
import DataExplorer from "./pages/DataExplorer";

type Page = "dashboard" | "sync" | "gallery" | "data";

const pages: Array<{ id: Page; label: string }> = [
  { id: "dashboard", label: "Dashboard" },
  { id: "sync", label: "Sync" },
  { id: "gallery", label: "Gallery" },
  { id: "data", label: "Data" },
];

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");

  useEffect(() => {
    document.title = "PhoneBridge";
  }, []);

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
      </aside>
      <main className="content">
        {page === "dashboard" && <Dashboard />}
        {page === "sync" && <Sync />}
        {page === "gallery" && <Gallery />}
        {page === "data" && <DataExplorer />}
      </main>
    </div>
  );
}
