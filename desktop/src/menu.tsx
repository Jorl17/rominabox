import { useId, useState } from "react";

export type ThemeId = "megadrive" | "8-bit";
const letters: Record<string, string> = {
  A: "01110100011000111111100011000110001",
  B: "11110100011000111110100011000111110",
  C: "01111100001000010000100001000001111",
  D: "11110100011000110001100011000111110",
  E: "11111100001000011110100001000011111",
  F: "11111100001000011110100001000010000",
  G: "01111100001000010111100011000101111",
  H: "10001100011000111111100011000110001",
  I: "11111001000010000100001000010011111",
  J: "00111000100001000010100101001001100",
  K: "10001100101010011000101001001010001",
  L: "10000100001000010000100001000011111",
  M: "10001110111010110101100011000110001",
  N: "10001110011010110011100011000110001",
  O: "01110100011000110001100011000101110",
  P: "11110100011000111110100001000010000",
  Q: "01110100011000110001101011001001101",
  R: "11110100011000111110101001001010001",
  S: "01111100001000001110000010000111110",
  T: "11111001000010000100001000010000100",
  U: "10001100011000110001100011000101110",
  V: "10001100011000110001100010101000100",
  W: "10001100011000110101101011101110001",
  X: "10001100010101000100010101000110001",
  Y: "10001100010101000100001000010000100",
  Z: "11111000010001000100010001000011111",
  "0": "01110100011001110101110011000101110",
  "1": "00100011000010000100001000010001110",
  "2": "01110100010000100010001000100011111",
  "3": "11110000010000101110000010000111110",
  "4": "00010001100101010010111110001000010",
  "5": "11111100001000011110000010000111110",
  "6": "01110100001000011110100011000101110",
  "7": "11111000010001000100010000100001000",
  "8": "01110100011000101110100011000101110",
  "9": "01110100011000101111000010000101110",
  "-": "00000000000000011111000000000000000",
  ":": "00000001000010000000001000010000000",
  ".": "00000000000000000000000000010000100",
  "?": "01110100010000100010001000000000100",
  " ": "00000000000000000000000000000000000",
};
/** Original bitmap lettering, shared by themes without shipping a font runtime. */
function PixelText({
  children,
  banded = false,
}: {
  children: string;
  banded?: boolean;
}) {
  const id = useId();
  const text = children
    .toUpperCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "");
  const path = [...text]
    .flatMap((letter, i) =>
      [...(letters[letter] || letters["?"])].map((pixel, j) =>
        pixel === "1" ? `M${i * 6 + (j % 5)},${Math.floor(j / 5)}h1v1h-1z` : "",
      ),
    )
    .join("");
  return (
    <svg
      className="pixel-text"
      viewBox={`0 0 ${Math.max(1, text.length * 6)} 9`}
      role="img"
      aria-label={children}
      shapeRendering="crispEdges"
    >
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffffff" />
          <stop offset=".43" stopColor="#ffffff" />
          <stop offset=".44" stopColor="#ffea24" />
          <stop offset="1" stopColor="#ff941c" />
        </linearGradient>
      </defs>
      <path d={path} fill="#06132d" transform="translate(1 1)" />
      <path d={path} fill={banded ? `url(#${id})` : "currentColor"} />
    </svg>
  );
}
function Landscape({ variant = 0 }: { variant?: number }) {
  return (
    <svg
      viewBox="0 0 160 104"
      preserveAspectRatio="xMidYMid slice"
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      <rect
        width="160"
        height="104"
        fill={variant === 1 ? "#252779" : "#1988ef"}
      />
      <path
        d="M0 38h8v-5h16v5h8v5H0zm92-16h10v-5h18v5h15v6H92"
        fill="#c0f7ff"
      />
      <path
        d="M0 80V62h10v-9h12v-9h12v10h12v10h12v16zm75 0V68h10V52h13V40h13v12h13v16h12v12z"
        fill="#1656bf"
      />
      <path d="M0 84h160v20H0z" fill="#763d26" />
      <path d="M0 82h160v8H0z" fill="#173e1a" />
      <path d="M0 78h160v7H0z" fill="#23ba45" />
      <path d="M0 76h160v3H0z" fill="#abec24" />
      {Array.from({ length: 10 }, (_, i) => (
        <path key={i} d={`M${i * 16} 93h8v5h-8zm8 6h8v5h-8z`} fill="#ba7531" />
      ))}
      <path d="M115 77V36h5v41z" fill="#ab722e" />
      <path
        d="M116 36h-7v-7H91v-5h23v5h5v-9h16v5h-10v5h14v6h-18v8h-5z"
        fill="#b2e626"
      />
      <path d="M46 75v-8h4v-4h7v4h4v8z" fill="#fff1b1" />
      <path d="M48 73h11v5H48z" fill="#fa4848" />
      <path d="M54 62v-5h7v5z" fill="#ffe318" />
      {[23, 73, 91].map((x) => (
        <path
          key={x}
          d={`M${x} 55h5v2h2v7h-2v2h-5v-2h-2v-7h2zm0 2v7h5v-7z`}
          fill="#ffdf19"
          fillRule="evenodd"
        />
      ))}
    </svg>
  );
}
type MenuProps = { title: string; theme: ThemeId; interactive: boolean };
type Page = "saves" | "options" | "game";
/** Both layouts share one preview interaction model; no emulator or real save files are touched. */
export function GameMenu({ title, theme, interactive }: MenuProps) {
  const [slot, setSlot] = useState(0);
  const [page, setPage] = useState<Page>("saves");
  const [filter, setFilter] = useState("Original");
  const [sound, setSound] = useState(true);
  const select = (n: number) => {
    if (interactive) setSlot((n + 6) % 6);
  };
  const open = (p: Page) => {
    if (interactive) setPage(p);
  };
  return (
    <div
      className={`game-menu ${theme === "megadrive" ? "sixteen" : "eight"}`}
      tabIndex={interactive ? 0 : -1}
      onKeyDown={(e) => {
        if (!interactive || page !== "saves") return;
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          e.preventDefault();
          select(slot + (e.key === "ArrowRight" ? 1 : -1));
        }
      }}
    >
      <div className="game-topline">
        <span>ROM-IN-A-BOX</span>
        <span>{page === "game" ? "GAME PREVIEW" : "PLAYER 1"}</span>
      </div>
      <div className="game-title">
        <PixelText banded={theme === "megadrive"}>
          {page === "options"
            ? "OPTIONS"
            : page === "game"
              ? "STARLIGHT TRAIL"
              : "SAVE SELECT"}
        </PixelText>
      </div>
      <div className="game-subtitle">
        <PixelText>{title.slice(0, 28) || "YOUR GAME"}</PixelText>
      </div>
      {page === "saves" ? (
        <>
          {theme === "megadrive" ? (
            <div className="save-carousel">
              <button
                className="pixel-arrow"
                aria-label="Previous slot"
                disabled={!interactive}
                onClick={() => select(slot - 1)}
              >
                ◀
              </button>
              {[-1, 0, 1].map((offset) => {
                const n = (slot + offset + 6) % 6;
                return (
                  <button
                    key={offset}
                    className={`save-panel ${offset === 0 ? "selected" : ""}`}
                    disabled={!interactive}
                    onClick={() => (offset === 0 ? open("game") : select(n))}
                    aria-label={`Slot ${n + 1}${n < 2 ? ", example save" : ", empty"}`}
                  >
                    <div className="slot-label">
                      <PixelText>{`FILE ${String(n + 1).padStart(2, "0")}`}</PixelText>
                    </div>
                    <div className="save-picture">
                      {n < 2 ? (
                        <Landscape variant={n} />
                      ) : (
                        <span className="empty-slot">+</span>
                      )}
                    </div>
                    <div className="slot-caption">
                      <PixelText>{n < 2 ? "CONTINUE" : "NEW GAME"}</PixelText>
                      <span>
                        {n < 2 ? (n === 0 ? "00:18:42" : "00:06:15") : "— — —"}
                      </span>
                    </div>
                  </button>
                );
              })}
              <button
                className="pixel-arrow"
                aria-label="Next slot"
                disabled={!interactive}
                onClick={() => select(slot + 1)}
              >
                ▶
              </button>
            </div>
          ) : (
            <div className="eight-grid">
              {Array.from({ length: 6 }, (_, n) => (
                <button
                  key={n}
                  className={`eight-slot ${slot === n ? "selected" : ""}`}
                  disabled={!interactive}
                  onClick={() => {
                    select(n);
                    open("game");
                  }}
                >
                  <b>{String(n + 1).padStart(2, "0")}</b>
                  <span>
                    {n < 2 ? (
                      <Landscape variant={n} />
                    ) : (
                      <span className="empty-slot">+</span>
                    )}
                  </span>
                  <strong>{n < 2 ? "CONTINUE" : "EMPTY"}</strong>
                </button>
              ))}
            </div>
          )}
          <div className="game-actions">
            <button disabled={!interactive} onClick={() => open("options")}>
              <PixelText>OPTIONS</PixelText>
            </button>
            <span className="slot-counter">
              {String(slot + 1).padStart(2, "0")} / 06
            </span>
            <button disabled={!interactive} onClick={() => open("game")}>
              <PixelText>START GAME</PixelText>
            </button>
          </div>
        </>
      ) : page === "options" ? (
        <div className="game-options">
          <button
            onClick={() =>
              setFilter(filter === "Original" ? "Soft CRT" : "Original")
            }
          >
            <PixelText>FILTER</PixelText>
            <span>◀ {filter.toUpperCase()} ▶</span>
          </button>
          <button onClick={() => setSound(!sound)}>
            <PixelText>SOUND</PixelText>
            <span>◀ {sound ? "ON" : "OFF"} ▶</span>
          </button>
          <div className="control-row">
            <PixelText>CONTROLS</PixelText>
            <span>← ↑ ↓ → &nbsp; Z X C</span>
          </div>
          <button className="return-menu" onClick={() => open("saves")}>
            <PixelText>BACK</PixelText>
          </button>
        </div>
      ) : (
        <div className="gameplay-preview">
          <Landscape />
          <div>Emulator not connected</div>
          <button onClick={() => open("saves")}>Return to menu</button>
        </div>
      )}
      <div className="game-foot">
        <span>
          {page === "saves" ? "SELECT A FILE · PRESS START" : "ROM-IN-A-BOX"}
        </span>
        <span>v0.1</span>
      </div>
    </div>
  );
}
