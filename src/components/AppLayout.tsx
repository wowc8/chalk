import { useState } from "react";
import { Outlet, useNavigate, useLocation } from "react-router-dom";
import { AppHeader } from "./AppHeader";
import { SettingsPanel } from "./SettingsPanel";
import { UpdateBanner } from "./UpdateBanner";

export function AppLayout({ onReconnect }: { onReconnect: () => void }) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();

  const isPlanDetail = location.pathname.startsWith("/plan/");

  return (
    <div className="min-h-screen chalk-bg text-chalk-white flex flex-col relative overflow-hidden">
      {/* Chalk grid overlay */}
      <div className="absolute inset-0 chalk-grid pointer-events-none" />

      {/* Chalk dust particles — decorative */}
      <div className="absolute inset-0 pointer-events-none overflow-hidden">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="chalk-dust-particle"
            style={{
              left: `${15 + i * 14}%`,
              bottom: `${5 + (i % 3) * 10}%`,
              animationDelay: `${i * 1.2}s`,
              animationDuration: `${5 + i * 0.8}s`,
            }}
          />
        ))}
      </div>

      <div className="relative z-10 flex flex-col min-h-screen">
        <UpdateBanner />
        <AppHeader
          onOpenSettings={() => setSettingsOpen(true)}
          breadcrumb={
            isPlanDetail
              ? { label: "Library", onClick: () => navigate("/") }
              : undefined
          }
        />
        <Outlet />
      </div>

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onReconnect={() => {
          setSettingsOpen(false);
          onReconnect();
        }}
      />
    </div>
  );
}
