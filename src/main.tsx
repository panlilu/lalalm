import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// Note: intentionally not using StrictMode — its double-effect simulation
// would unsubscribe our single-fire Tauri event listeners in dev builds.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.Fragment>
    <App />
  </React.Fragment>
);
