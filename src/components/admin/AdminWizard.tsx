import { useState, useEffect } from "react";
import {
  useAdminSetup,
  type DriveFolder,
  type SetupStep,
} from "../../hooks/useAdminSetup";
import "./AdminWizard.css";

const STEP_LABELS: Record<SetupStep, string> = {
  welcome: "Welcome",
  credentials: "API Credentials",
  authorize: "Authorize",
  folder: "Select Folder",
  shred: "Initial Scan",
  complete: "All Set",
};

const STEP_ORDER: SetupStep[] = [
  "welcome",
  "credentials",
  "authorize",
  "folder",
  "shred",
  "complete",
];

export function AdminWizard() {
  const {
    step,
    setStep,
    status,
    error,
    loading,
    saveCredentials,
    getAuthUrl,
    submitAuthCode,
    listFolders,
    selectFolder,
    triggerShred,
  } = useAdminSetup();

  return (
    <div className="admin-wizard">
      <header className="wizard-header">
        <h1>Chalk Setup</h1>
        <p className="wizard-subtitle">
          Let's connect your Google Drive so Chalk can read your lesson plans.
        </p>
      </header>

      <nav className="wizard-progress">
        {STEP_ORDER.map((s) => (
          <span
            key={s}
            className={`progress-dot ${s === step ? "active" : ""} ${
              STEP_ORDER.indexOf(s) < STEP_ORDER.indexOf(step) ? "done" : ""
            }`}
          >
            {STEP_LABELS[s]}
          </span>
        ))}
      </nav>

      {error && <div className="wizard-error">{error}</div>}

      <div className="wizard-content">
        {step === "welcome" && <WelcomeStep onNext={() => setStep("credentials")} />}
        {step === "credentials" && (
          <CredentialsStep onSubmit={saveCredentials} loading={loading} />
        )}
        {step === "authorize" && (
          <AuthorizeStep
            getAuthUrl={getAuthUrl}
            onSubmitCode={submitAuthCode}
            loading={loading}
          />
        )}
        {step === "folder" && (
          <FolderStep
            listFolders={listFolders}
            onSelect={selectFolder}
            loading={loading}
          />
        )}
        {step === "shred" && (
          <ShredStep
            folderName={status?.selected_folder_name ?? "your folder"}
            onTrigger={triggerShred}
            loading={loading}
          />
        )}
        {step === "complete" && <CompleteStep />}
      </div>
    </div>
  );
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="step-card">
      <h2>Welcome to Chalk</h2>
      <p>
        Chalk connects to your Google Drive to read your existing lesson plans,
        then uses AI to help you create new ones that match your style.
      </p>
      <p>This quick setup will:</p>
      <ol>
        <li>Connect your Google account (read-only access)</li>
        <li>Choose the folder with your lesson plans</li>
        <li>Scan your existing documents</li>
      </ol>
      <button className="btn-primary" onClick={onNext}>
        Get Started
      </button>
    </div>
  );
}

function CredentialsStep({
  onSubmit,
  loading,
}: {
  onSubmit: (clientId: string, clientSecret: string) => void;
  loading: boolean;
}) {
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");

  return (
    <div className="step-card">
      <h2>Google API Credentials</h2>
      <p>
        Enter your Google Cloud OAuth credentials. These are used to securely
        connect Chalk to your Google Drive.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit(clientId, clientSecret);
        }}
      >
        <label>
          Client ID
          <input
            type="text"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            placeholder="your-client-id.apps.googleusercontent.com"
            required
          />
        </label>
        <label>
          Client Secret
          <input
            type="password"
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            placeholder="Client secret"
            required
          />
        </label>
        <button className="btn-primary" type="submit" disabled={loading}>
          {loading ? "Saving..." : "Save & Continue"}
        </button>
      </form>
    </div>
  );
}

function AuthorizeStep({
  getAuthUrl,
  onSubmitCode,
  loading,
}: {
  getAuthUrl: () => Promise<string | null>;
  onSubmitCode: (code: string) => void;
  loading: boolean;
}) {
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [code, setCode] = useState("");

  useEffect(() => {
    getAuthUrl().then(setAuthUrl);
  }, [getAuthUrl]);

  return (
    <div className="step-card">
      <h2>Authorize Google Access</h2>
      <p>
        Click the link below to sign in with Google. After granting access,
        paste the authorization code here.
      </p>
      {authUrl && (
        <a
          className="auth-link"
          href={authUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          Open Google Sign-In
        </a>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSubmitCode(code);
        }}
      >
        <label>
          Authorization Code
          <input
            type="text"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="Paste the code from Google here"
            required
          />
        </label>
        <button className="btn-primary" type="submit" disabled={loading}>
          {loading ? "Verifying..." : "Submit Code"}
        </button>
      </form>
    </div>
  );
}

function FolderStep({
  listFolders,
  onSelect,
  loading,
}: {
  listFolders: () => Promise<DriveFolder[]>;
  onSelect: (id: string, name: string) => void;
  loading: boolean;
}) {
  const [folders, setFolders] = useState<DriveFolder[]>([]);
  const [fetching, setFetching] = useState(false);

  useEffect(() => {
    setFetching(true);
    listFolders()
      .then(setFolders)
      .finally(() => setFetching(false));
  }, [listFolders]);

  return (
    <div className="step-card">
      <h2>Select Your Lesson Plans Folder</h2>
      <p>Choose the Google Drive folder that contains your master lesson plan documents.</p>
      {fetching ? (
        <p className="loading-text">Loading folders...</p>
      ) : folders.length === 0 ? (
        <p>No folders found. Make sure your Google Drive has folders.</p>
      ) : (
        <ul className="folder-list">
          {folders.map((f) => (
            <li key={f.id}>
              <button
                className="folder-btn"
                onClick={() => onSelect(f.id, f.name)}
                disabled={loading}
              >
                {f.name}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function ShredStep({
  folderName,
  onTrigger,
  loading,
}: {
  folderName: string;
  onTrigger: () => void;
  loading: boolean;
}) {
  return (
    <div className="step-card">
      <h2>Scan Your Documents</h2>
      <p>
        Chalk will now scan <strong>{folderName}</strong> to discover and index
        your existing lesson plans.
      </p>
      <button className="btn-primary" onClick={onTrigger} disabled={loading}>
        {loading ? "Scanning..." : "Start Scan"}
      </button>
    </div>
  );
}

function CompleteStep() {
  return (
    <div className="step-card">
      <h2>Setup Complete</h2>
      <p>
        Chalk is connected to your Google Drive and has scanned your lesson
        plans. You're ready to start creating.
      </p>
    </div>
  );
}

export default AdminWizard;
