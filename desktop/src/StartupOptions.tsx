import { Checkbox } from "./Help";
import type { Draft } from "./App";

// What happens when the game starts and stops, on the details step.
export function StartupOptions({
  draft,
  update,
  startAtMenu,
}: {
  draft: Draft;
  update: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  startAtMenu: boolean;
}) {
  return (
    <>
      <Checkbox
        className="splash-choice"
        label="Startup logo"
        checked={draft.splash}
        onChange={(value) => update("splash", value)}
        help="Show a brief ROM-in-a-Box logo in the game window at startup."
      />
      <Checkbox
        label="Achievements"
        checked={draft.showMenu && draft.includeAchievements}
        disabled={!draft.showMenu}
        onChange={(value) => update("includeAchievements", value)}
        help={
          draft.showMenu
            ? "Let the player sign in to RetroAchievements and earn achievements."
            : "Requires game menu."
        }
      />
      <Checkbox
        label="Keep playing in the background"
        checked={draft.keepPlayingInBackground}
        onChange={(value) => update("keepPlayingInBackground", value)}
        help="Let the game keep running when its window is not in front. When off, it pauses until the player returns."
      />
      <Checkbox
        label="Autosave on quit"
        checked={draft.autosaveOnQuit}
        onChange={(value) => update("autosaveOnQuit", value)}
        help="Save the game when the player quits, and continue from there at the next launch."
      />
      {startAtMenu && (
        <Checkbox
          label="Show menu at startup"
          checked={draft.startAtMenu}
          onChange={(value) => update("startAtMenu", value)}
          help="Start at the menu before playing. The menu is also available during play."
        />
      )}
    </>
  );
}
