import { useEffect, useState } from "react";
import appIcon from "../src-tauri/icons/icon.png";
import { AboutDialog } from "./AboutDialog";
import { BuilderUpdateButton } from "./BuilderUpdateButton";
import { native, onAboutRequested } from "./bridge";
import { GameData } from "./GameData";

/** The band across the top: the wordmark, the edition, the link between the
 * builder and the Installed games section, and About. The About dialog is outside
 * the band so it does not have the colours of the band. We show the section
 * after the band, in place of the builder's steps. */
export function AppHeader() {
  const [about, setAbout] = useState(false);
  const [gameData, setGameData] = useState(false);
  // About in the macOS menu bar opens the same dialog.
  useEffect(() => {
    if (!native) return;
    const stop = onAboutRequested(() => setAbout(true));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, []);
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
          <BuilderUpdateButton />
          <button
            className="text-button header-link"
            aria-pressed={gameData}
            onClick={() => setGameData(!gameData)}
          >
            {gameData ? "Builder" : "Installed games"}
          </button>
          <button
            className="text-button about-button"
            onClick={() => setAbout(true)}
          >
            About
          </button>
        </div>
      </header>
      {gameData && <GameData />}
      {about && <AboutDialog onClose={() => setAbout(false)} />}
    </>
  );
}
