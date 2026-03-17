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
    <div className="h-screen chalk-bg text-chalk-white flex flex-col overflow-hidden">
      <UpdateBanner />
      <AppHeader
        onOpenSettings={() => setSettingsOpen(true)}
        breadcrumb={
          isPlanDetail
            ? { label: "Library", onClick: () => navigate("/") }
            : undefined
        }
      />
      <main className="flex-1 overflow-y-auto">
        <Outlet />
      </main>

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
