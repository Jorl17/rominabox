import declared from "../../vendor/retroarch/menu/drivers/rmlui/mouse_buttons.inc?raw";

/**
 * The mouse buttons for a control, as declared for the game
 * (mouse_buttons.inc): the value for each in a controls file, which we store
 * in the builder, and the word for it in the game.
 */
export const mouseButtons: { value: string; word: string }[] = [
  ...declared.matchAll(
    /^RIB_MOUSE_BUTTON\("([^"]+)", RETRO_DEVICE_ID_MOUSE_\w+, "([^"]*)"\)$/gm,
  ),
].map(([, value, word]) => ({ value, word }));
