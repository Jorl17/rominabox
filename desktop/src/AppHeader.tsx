import { useState } from "react";
import appIcon from "../src-tauri/icons/icon.png";
import { AboutDialog } from "./AboutDialog";
import { native } from "./bridge";

/** The band across the top: the wordmark, the edition, and About. The About
 * dialog is outside the band so it does not have the colours of the band. */
export function AppHeader() {
  const [about, setAbout] = useState(false);
  return (
    <>
      <header className="app-header">
        <div className="wordmark">
          <img className="brand-mark" src={appIcon} alt="" />
          <span>ROM-in-a-Box</span>
        </div>
        <div className="header-end">
          <span className="edition">
            {native ? "GAME APP BUILDER" : "BROWSER PREVIEW"}
          </span>
          <button
            className="text-button about-button"
            onClick={() => setAbout(true)}
          >
            About
          </button>
        </div>
      </header>
      {about && <AboutDialog onClose={() => setAbout(false)} />}
    </>
  );
}
