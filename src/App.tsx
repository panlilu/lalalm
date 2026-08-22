import { useLayoutEffect, useRef } from "react";
import { StoreProvider, useStore } from "./store";
import { Sidebar } from "./components/Sidebar";
import { Discover } from "./pages/Discover";
import { ModelDetail } from "./pages/ModelDetail";
import { OnDevice } from "./pages/OnDevice";
import { Downloads } from "./pages/Downloads";
import { Settings } from "./pages/Settings";

function CurrentPage() {
  const { route } = useStore();
  switch (route.page) {
    case "discover":
      return <Discover />;
    case "detail":
      return <ModelDetail repo={route.repo} source={route.source} />;
    case "device":
      return <OnDevice />;
    case "downloads":
      return <Downloads />;
    case "settings":
      return <Settings />;
  }
}

function Shell() {
  const { route } = useStore();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // Remember the scroll offset of every page (incl. per-repo detail pages)
  // so switching tabs restores exactly where the user left off.
  const posMap = useRef<Map<string, number>>(new Map());
  const pageKey =
    route.page === "detail"
      ? `detail:${route.source}:${route.repo}`
      : route.page;
  const keyRef = useRef(pageKey);
  keyRef.current = pageKey;

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = posMap.current.get(keyRef.current) ?? 0;
  }, [pageKey]);

  return (
    <div className="app-shell">
      <Sidebar />
      <main className="main-area">
        <div
          ref={scrollRef}
          className="page"
          onScroll={(e) => {
            const el = e.currentTarget;
            posMap.current.set(keyRef.current, el.scrollTop);
          }}
        >
          <CurrentPage />
        </div>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}
