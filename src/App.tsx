import { useState } from "react";
import TeacherView from "./TeacherView";
import StudentView from "./StudentView";
import LogPanel from "./LogPanel";
import "./App.css";

function App() {
  const [role, setRole] = useState<"teacher" | "student" | null>(null);

  return (
    <div className="container" style={{ paddingBottom: '160px' }}>
      <h1>Screen Sharing App</h1>

      {!role && (
        <div className="card">
          <p>Select your role:</p>
          <div style={{ display: 'flex', gap: '20px', justifyContent: 'center' }}>
            <button onClick={() => setRole("teacher")}> Teacher (Share Screen)</button>
            <button onClick={() => setRole("student")}> Student (View Screen)</button>
          </div>
        </div>
      )}

      {role === "teacher" && (
        <div>
          <button onClick={() => setRole(null)} style={{ float: 'left' }}>Back</button>
          <TeacherView />
        </div>
      )}

      {role === "student" && (
        <div>
          <button onClick={() => setRole(null)} style={{ float: 'left' }}>Back</button>
          <StudentView />
        </div>
      )}
      <LogPanel />
    </div>
  );
}

export default App;
