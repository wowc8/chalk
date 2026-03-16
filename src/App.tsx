import { useErrorPipe } from "./hooks/useErrorPipe";
import { AdminWizard } from "./components/admin/AdminWizard";
import "./App.css";

function App() {
  useErrorPipe();

  return (
    <main className="container">
      <AdminWizard />
    </main>
  );
}

export default App;
