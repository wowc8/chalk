import { motion } from "framer-motion";

interface AppHeaderProps {
  onOpenSettings: () => void;
  breadcrumb?: { label: string; onClick: () => void };
  title?: string;
}

export function AppHeader({ onOpenSettings, breadcrumb, title }: AppHeaderProps) {
  return (
    <header className="sticky top-0 z-30 bg-chalk-board-dark/90 backdrop-blur-sm border-b border-chalk-white/5">
      <div className="max-w-6xl mx-auto px-6 h-14 flex items-center justify-between">
        {/* Left side: breadcrumb or app name */}
        <div className="flex items-center gap-3">
          {breadcrumb ? (
            <button
              onClick={breadcrumb.onClick}
              className="flex items-center gap-1.5 text-sm text-chalk-muted hover:text-chalk-white transition-colors group"
            >
              <svg
                className="w-4 h-4 transition-transform group-hover:-translate-x-0.5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
              <span>{breadcrumb.label}</span>
            </button>
          ) : (
            <h1 className="chalk-heading text-xl tracking-wide text-chalk-white">
              Chalk
            </h1>
          )}
          {title && (
            <>
              {breadcrumb && <span className="text-chalk-muted/40">/</span>}
              <span className="text-sm font-medium text-chalk-white truncate max-w-xs">
                {title}
              </span>
            </>
          )}
        </div>

        {/* Right side: settings cog */}
        <motion.button
          whileHover={{ scale: 1.1, rotate: 90 }}
          whileTap={{ scale: 0.9 }}
          transition={{ type: "spring", stiffness: 300, damping: 20 }}
          onClick={onOpenSettings}
          className="p-2 rounded-lg text-chalk-muted hover:text-chalk-white transition-colors"
          title="Settings"
          aria-label="Open settings"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
            />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </motion.button>
      </div>
    </header>
  );
}
