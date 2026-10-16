import React from "react";
import ReactDOM from "react-dom/client";

import { initializePreferences } from "./lib/prefs";
import "./index.css";

// The view's own right-click menu offers "Reload" and "Inspect Element",
// neither of which belongs in an app. Photos have a menu of their own.
window.addEventListener("contextmenu", (e) => {
  if (!import.meta.env.DEV) e.preventDefault();
});

// A button clicked with the mouse does not take the keyboard.
//
// Left focused, the next press of Space or Return would press it again — and
// in this app those keys are a photographer's, mid-cull. Refusing the focus as
// the mouse goes down is the whole fix: the click still happens, and nothing
// has to be un-focused afterwards. (Un-focusing afterwards was tried first. It
// also un-focused the dialog a button had just opened, which the dialog's own
// focus handling then fought with.) Dialogs and menus are left alone, where
// moving between controls by keyboard is the point.
window.addEventListener(
  "mousedown",
  (e) => {
    const button = (e.target as HTMLElement | null)?.closest("button, [role='button']");
    if (button && !button.closest("[role='dialog'], [role='alertdialog'], [role='menu']")) e.preventDefault();
  },
  true,
);

await initializePreferences();
const { default: App } = await import("./App");
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
